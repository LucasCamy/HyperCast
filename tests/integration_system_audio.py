"""System loopback -> Opus -> SFU -> decoded stereo -> WASAPI output.

Plays a quiet generated tone from a separate FFplay process. No recording.
Receiver renders zeros to prevent feedback between two clients on one PC.
"""
import json
import os
from pathlib import Path
import re
import subprocess
import time
import urllib.request
import uuid
from tools import room_token as rt

ROOT = Path(__file__).resolve().parent.parent

def main():
    key,secret = rt.load_env(ROOT/'.tools/homelab.env')
    room = 'audio-'+uuid.uuid4().hex[:12]; now = int(time.time())
    def token(publisher):
        grants = {'room':room,'roomJoin':True,'canPublish':publisher,'canSubscribe':True,'canPublishData':False}
        if publisher: grants['canPublishSources'] = ['screen_share','screen_share_audio']
        return rt.sign({'iss':key,'sub':'hc-member-'+uuid.uuid4().hex,'nbf':now-5,'exp':now+300,'video':grants},secret)
    base = {k:v for k,v in os.environ.items() if k not in {'LIVEKIT_API_KEY','LIVEKIT_API_SECRET','LIVEKIT_KEYS'}}
    relay_only = os.environ.get('HYPERCAST_AUDIO_TEST_RELAY_ONLY','1') == '1'
    base.update(HYPERCAST_URL='wss://hypercast.cloudlane.com.br',HYPERCAST_AUTO_CONNECT='1',HYPERCAST_RELAY_ONLY='1' if relay_only else '0')
    executable = os.environ.get('HYPERCAST_TEST_EXE',str(ROOT/'target/x86_64-pc-windows-msvc/debug/hypercast.exe'))
    children=[]; outputs=[]
    diagnostics = Path(os.environ['LOCALAPPDATA'])/'HyperCast/diagnostics'
    before = set(diagnostics.glob('gui-*.log'))
    try:
        # The tone is the only generated audio; no audio files are created.
        tone = subprocess.Popen([str(ROOT/'.tools/ffmpeg/bin/ffplay.exe'),'-nodisp','-autoexit','-volume','10',
            '-f','lavfi','-i','aevalsrc=0.02*sin(2*PI*997*t)|0.02*sin(2*PI*1477*t):s=48000:d=22','-t','22'],stdout=subprocess.DEVNULL,stderr=subprocess.DEVNULL)
        children.append(tone)
        for publisher, label, duration in [(False,'viewer',20),(True,'publisher',18)]:
            output=(ROOT/'.tools'/f'audio-{label}-child.log').open('wb'); outputs.append(output)
            env={**base,'HYPERCAST_ROOM_TOKEN':token(publisher),'HYPERCAST_SMOKE_SECONDS':str(duration)}
            if publisher: env.update(HYPERCAST_SMOKE_SHARE='1',HYPERCAST_SYSTEM_AUDIO='1')
            else: env.update(HYPERCAST_AUDIO_TEST_SILENT_OUTPUT='1',HYPERCAST_SMOKE_SOUND='1')
            child=subprocess.Popen([executable,'--gui'],cwd=ROOT,env=env,stdout=output,stderr=subprocess.STDOUT)
            children.append(child)
            time.sleep(1)
        for child in children[1:]: assert child.wait(timeout=40)==0,'GUI failed'
        for output in outputs: output.close()
        target_pids={child.pid for child in children[1:]}
        logs=[path.read_text(errors='replace') for path in set(diagnostics.glob('gui-*.log'))-before
            if any(path.name.endswith(f'-{pid}.log') for pid in target_pids)]
        assert len(logs)==2,'Missing session diagnostics'
        assert any('System audio published:' in log for log in logs),'No audio publication; inspect the child/session diagnostics'
        publisher=next(log for log in logs if 'System audio published:' in log)
        viewer=next(log for log in logs if 'Subscribed to system audio' in log)
        assert publisher.count('System audio published:')==2,'Stop/republication failed'
        assert 'Decoded non-silent system audio' in viewer,'No decoded audio signal'
        assert 'Decoded distinct stereo channels' in viewer,'Stereo was downmixed'
        assert 'WASAPI playback running' in viewer,'No real WASAPI output'
        snapshots = [(int(m[1]),int(m[2]),int(m[3])) for m in re.finditer(r'(\d+) audio_stats: sent=\d+ received=(\d+) receivers=(\d+)',viewer)]
        assert len(snapshots)>=3,'Missing audio subscription snapshots'
        changes=[(int(m[1]),int(m[2])) for m in re.finditer(r'(\d+) layout: .* volume=(\d+)',viewer)]
        mute=next(t for t,volume in changes if volume==0)
        resumed=next(t for t,volume in changes if t>mute and volume>0)
        muted=[(count,receivers) for t,count,receivers in snapshots if mute+250<t<resumed]
        assert muted and all(receivers==0 for _,receivers in muted),'Muted receiver remained subscribed'
        assert max((count for t,count,receivers in snapshots if t>resumed),default=0) > muted[-1][0]+100,'Audio did not resume'
        assert max(map(int,re.findall(r'audio_stats: sent=\d+ received=(\d+)',viewer)),default=0)>200,'Insufficient decoded audio'
        for log in logs:
            assert not re.search(r'audio_error:|audio_playback_error:|audio_unpublish_error:|publication_error:|capture_error:',log),'Audio/video pipeline error'
            if relay_only: assert 'relay transport=Some(Tls)' in log,'TURN TLS was not selected'
        print('PASS: system PCM capture, Opus, '+('TURN TLS' if relay_only else 'WebRTC')+', decoded non-silent stereo, WASAPI output, stop and republish')
    finally:
        for child in children:
            if child.poll() is None: child.terminate(); child.wait(timeout=10)
        for output in outputs: output.close()

if __name__=='__main__': main()
