"""Owned synthetic app -> WGC/AMD/AMF -> real SFU -> GPU desktop presentation."""
import os
from pathlib import Path
import re
import subprocess
import time
import uuid
from tools import room_token as rt
from tests.integration_desktop import owned_window,screenshot
from tests.integration_room_creation import cleanup
ROOT=Path(__file__).resolve().parent.parent

def main():
    adapter=os.environ['HYPERCAST_TEST_AMD_ADAPTER']
    assert adapter.isdigit() and int(adapter)<16
    key,secret=rt.load_env(ROOT/'.tools/homelab.env');room='amf-'+uuid.uuid4().hex[:12]
    now=int(time.time())
    def token(pub):
        return rt.sign({'iss':key,'sub':'hc-member-'+uuid.uuid4().hex,'nbf':now-5,'exp':now+180,
            'video':{'room':room,'roomJoin':True,'canPublish':pub,'canSubscribe':True,
            'canPublishData':False,'canPublishSources':['screen_share','screen_share_audio']}},secret)
    env={k:v for k,v in os.environ.items() if not k.startswith(('LIVEKIT_','HYPERCAST_','GH_','GITHUB_'))}
    env.update(HYPERCAST_URL='wss://hypercast.cloudlane.com.br',HYPERCAST_AUTO_CONNECT='1',HYPERCAST_CODEC='h265')
    exe=Path(os.environ.get('HYPERCAST_TEST_EXE',str(ROOT/'target/x86_64-pc-windows-msvc/debug/hypercast.exe')))
    children=[];files=[]
    try:
        app=subprocess.Popen([str(ROOT/'.tools/ffmpeg/bin/ffplay.exe'),'-window_title','HyperCast AMD synthetic',
            '-x','640','-y','360','-f','lavfi','-i','testsrc2=size=640x360:rate=60'],
            stdout=subprocess.DEVNULL,stderr=subprocess.DEVNULL);children.append(app)
        time.sleep(2);owned_window(app.pid)
        for pub,label,duration in [(False,'viewer',18),(True,'publisher',15)]:
            file=(ROOT/'.tools'/f'amf-window-{label}.log').open('wb');files.append(file)
            child_env={**env,'HYPERCAST_ROOM_TOKEN':token(pub),'HYPERCAST_SMOKE_SECONDS':str(duration)}
            if pub:child_env.update(HYPERCAST_SMOKE_SHARE='1',
                HYPERCAST_SMOKE_WINDOW_PID=str(app.pid),HYPERCAST_SMOKE_WINDOW_ADAPTER=adapter)
            child=subprocess.Popen([str(exe),'--gui'],cwd=ROOT,env=child_env,stdout=file,stderr=subprocess.STDOUT)
            children.append(child);time.sleep(1)
        time.sleep(8);screenshot(children[1].pid,ROOT/'.tools/amf-window-received.png')
        for child in children[1:]:assert child.wait(timeout=25)==0
        diagnostics=Path(os.environ['LOCALAPPDATA'])/'HyperCast/diagnostics'
        publisher=next(diagnostics.glob(f'gui-*-{children[2].pid}.log')).read_text(encoding='utf-8')
        assert ' encoder: AMF' in publisher,'Rust did not select the AMD encoder'
        assert 'encoder_window' in publisher and 'applied_bps=' in publisher,'Missing encoder quality measurements'
        assert 'video_rtp' in publisher,'Missing RTP quality counters'
        for file in files:file.close()
        viewer=(ROOT/'.tools/amf-window-viewer.log').read_text(encoding='utf-8')
        match=re.search(r'GUI smoke: (\d+) received, (\d+) GPU presentations',viewer)
        assert match and int(match[1])>10 and int(match[2])>10,'AMD stream did not decode/present'
        assert 'connected=true' in viewer
        assert 'GUI shutdown: 0 retained GPU decoder surfaces' in viewer,'GPU decoder surfaces survived shutdown'
        print('PASS: synthetic WGC -> Rust AMD/AMF HEVC -> SFU -> GPU desktop presentation')
    finally:
        for child in children:
            if child.poll() is None:child.terminate();child.wait(timeout=10)
        for file in files:file.close()
        cleanup(room,key,secret)

if __name__=='__main__':main()
