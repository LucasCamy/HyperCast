"""Real RTP: repeated GUI sharing must free server tracks without reconnecting.

Uses a unique room, no screen recording. Operator credentials stay in this
test process; children receive only a five-minute room JWT.
"""
import json
import os
from pathlib import Path
import subprocess
import time
import urllib.request
import uuid
from tools import room_token as rt

ROOT = Path(__file__).resolve().parent.parent

def main():
    key, secret = rt.load_env(ROOT / '.tools/homelab.env')
    room = 'lifecycle-' + uuid.uuid4().hex[:12]
    now = int(time.time())
    token = rt.sign({'iss':key, 'sub':'hc-member-'+uuid.uuid4().hex,
        'nbf':now-5, 'exp':now+300, 'video':{'room':room, 'roomJoin':True,
        'canPublish':True, 'canSubscribe':True, 'canPublishData':False,
        'canPublishSources':['screen_share']}}, secret)
    admin = rt.sign({'iss':key, 'sub':'hypercast-operator', 'nbf':now-5,
        'exp':now+90, 'video':{'room':room,'roomAdmin':True}}, secret)
    env = {k:v for k,v in os.environ.items() if k not in {'LIVEKIT_API_KEY','LIVEKIT_API_SECRET','LIVEKIT_KEYS'}}
    env.update(HYPERCAST_URL='wss://hypercast.cloudlane.com.br', HYPERCAST_ROOM_TOKEN=token,
        HYPERCAST_AUTO_CONNECT='1', HYPERCAST_SMOKE_SHARE='1',
        HYPERCAST_SMOKE_SHARE_CYCLES='4', HYPERCAST_SMOKE_SECONDS='25', HYPERCAST_CODEC='h265')
    executable = Path(os.environ.get('HYPERCAST_TEST_EXE', str(ROOT/'target/x86_64-pc-windows-msvc/debug/hypercast.exe')))
    log = ROOT/'.tools/publication-lifecycle-child.log'
    snapshots = []
    started = time.monotonic()
    try:
        with log.open('wb') as output:
            child = subprocess.Popen([str(executable),'--gui'], env=env, cwd=ROOT, stdout=output, stderr=subprocess.STDOUT)
            while child.poll() is None:
                request = urllib.request.Request('https://hypercast.cloudlane.com.br/twirp/livekit.RoomService/ListParticipants',
                    data=json.dumps({'room':room}).encode(), headers={'Content-Type':'application/json',
                    'Authorization':'Bearer '+admin, 'User-Agent':'HyperCast-Operator/0.2'})
                class NoRedirect(urllib.request.HTTPRedirectHandler):
                    def redirect_request(self, *args, **kwargs): return None
                with urllib.request.build_opener(NoRedirect).open(request, timeout=5) as response:
                    members = json.load(response).get('participants', [])
                snapshots.append((time.monotonic()-started, len(members), sum(len(p.get('tracks', [])) for p in members)))
                assert time.monotonic()-started < 45, 'GUI timed out'
                time.sleep(.25)
        text = log.read_text(errors='replace')
        assert child.returncode == 0, 'GUI failed; inspect protected local log'
        assert text.count('Published screen track') == 4, 'Repeated publication failed; inspect local log'
        assert 'SFU publication rejected' not in text and 'Unsupported capture texture layout' not in text
        # All three stop windows must retain room membership and remove server tracks.
        for low, high in [(6,7.8),(12,13.8),(18,19.8)]:
            assert any(low < t < high and members == 1 and tracks == 0 for t,members,tracks in snapshots), f'SFU retained stopped track at {low}s'
        assert any(tracks == 1 for _,_,tracks in snapshots), 'No real SFU publication observed'
        print('PASS: four real RTP publications in one connection; all three stops removed SFU tracks')
    finally:
        if 'child' in locals() and child.poll() is None:
            child.terminate(); child.wait(timeout=10)

if __name__ == '__main__': main()
