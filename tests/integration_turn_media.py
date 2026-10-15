"""Synthetic video through public TURN/TLS; no desktop capture, ephemeral room."""
import json
import os
from pathlib import Path
import re
import subprocess
import time
import urllib.error
import urllib.request
import uuid
from tools import room_token as rt
from tests.integration_desktop import owned_window

ROOT = Path(__file__).resolve().parent.parent


def main():
    key, secret = rt.load_env(ROOT / '.tools/homelab.env')
    now = int(time.time())
    room = 'turn-test-' + uuid.uuid4().hex[:12]
    url = 'wss://hypercast.cloudlane.com.br'
    def token(publish):
        grants = {'room': room, 'roomJoin': True, 'canPublish': publish, 'canSubscribe': True, 'canPublishData': False}
        if publish:
            grants['canPublishSources'] = ['screen_share', 'screen_share_audio']
        return rt.sign({'iss': key, 'sub': 'hc-screen-1' if publish else 'hc-member-' + uuid.uuid4().hex,
            'nbf': now - 5, 'exp': now + 180, 'video': grants}, secret)
    base = {k: v for k, v in os.environ.items() if not k.startswith(('HYPERCAST_', 'LIVEKIT_', 'GH_', 'GITHUB_'))}
    windows = os.environ.get('HYPERCAST_WINDOWS_EXE', str(ROOT / 'dist/HyperCast-0.7.4-windows-x64/hypercast.exe'))
    child = pattern = None
    try:
        pattern = subprocess.Popen([str(ROOT / '.tools/ffmpeg/bin/ffplay.exe'), '-window_title',
            'HyperCast TURN test', '-x', '640', '-y', '360', '-an', '-f', 'lavfi',
            '-i', 'testsrc2=size=640x360:rate=60'], stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
        time.sleep(2)
        owned_window(pattern.pid)
        env = {**base, 'HYPERCAST_RELAY_ONLY': '1', 'HYPERCAST_URL': url, 'HYPERCAST_ROOM_TOKEN': token(True),
            'HYPERCAST_AUTO_CONNECT': '1', 'HYPERCAST_SMOKE_SHARE': '1', 'HYPERCAST_SMOKE_SECONDS': '25',
            'HYPERCAST_SMOKE_WINDOW_PID': str(pattern.pid), 'HYPERCAST_CODEC': 'hevc'}
        with (ROOT / '.tools/public-turn-publisher.log').open('wb') as log:
            child = subprocess.Popen([windows, '--gui'], env=env, stdout=log, stderr=subprocess.STDOUT)
            time.sleep(2)
            viewer = subprocess.run([windows, '--receive-url', url, '--seconds', '14'],
                env={**base, 'HYPERCAST_RELAY_ONLY': '1', 'HYPERCAST_ROOM_TOKEN': token(False)},
                capture_output=True, timeout=45, encoding='utf-8', errors='replace')
            (ROOT / '.tools/public-turn-receiver.log').write_text(viewer.stdout + viewer.stderr, encoding='utf-8')
            assert viewer.returncode == 0, 'TURN receiver failed; inspect scoped log'
            match = re.search(r'Receiver GPU: (\d+) D3D11 texture frames', viewer.stdout)
            assert match and int(match[1]) > 100, 'TURN did not deliver enough decoded video'
            assert 'local relay=true, relay transport=Some(Tls)' in viewer.stdout, 'Selected route was not TURN/TLS'
            assert child.wait(timeout=35) == 0, 'Synthetic publisher failed'
            print(f'PASS: public TURN/TLS, {match[1]} GPU decoded frames; both clients relay-only')
    finally:
        if child is not None and child.poll() is None:
            child.terminate()
            child.wait(timeout=10)
        if pattern is not None and pattern.poll() is None:
            pattern.terminate()
            pattern.wait(timeout=10)
        admin = rt.sign({'iss': key, 'sub': 'operator', 'nbf': now - 5, 'exp': now + 180,
            'video': {'room': room, 'roomCreate': True, 'roomAdmin': True}}, secret)
        request = urllib.request.Request('https://hypercast.cloudlane.com.br/twirp/livekit.RoomService/DeleteRoom',
            data=json.dumps({'room': room}).encode(), headers={'Authorization': 'Bearer ' + admin,
            'Content-Type': 'application/json', 'User-Agent': 'HyperCast-Operator/0.7'})
        class NoRedirect(urllib.request.HTTPRedirectHandler):
            def redirect_request(self, *args, **kwargs): return None
        try:
            with urllib.request.build_opener(NoRedirect).open(request, timeout=10): pass
        except urllib.error.HTTPError as error:
            if error.code != 404: raise


if __name__ == '__main__':
    main()
