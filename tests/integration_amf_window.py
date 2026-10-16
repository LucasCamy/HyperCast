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
    expected=os.environ.get('HYPERCAST_TEST_ENCODER','AMF');assert expected in {'AMF','NVENC'}
    adapter=os.environ.get('HYPERCAST_TEST_CAPTURE_ADAPTER',os.environ.get('HYPERCAST_TEST_AMD_ADAPTER','1'))
    assert adapter.isdigit() and int(adapter)<16
    key,secret=rt.load_env(ROOT/'.tools/homelab.env');room='amf-'+uuid.uuid4().hex[:12]
    now=int(time.time())
    def token(pub):
        return rt.sign({'iss':key,'sub':'hc-member-'+uuid.uuid4().hex,'nbf':now-5,'exp':now+180,
            'video':{'room':room,'roomJoin':True,'canPublish':pub,'canSubscribe':True,
            'canPublishData':False,'canPublishSources':['screen_share','screen_share_audio']}},secret)
    env={k:v for k,v in os.environ.items() if not k.startswith(('LIVEKIT_','HYPERCAST_','GH_','GITHUB_'))}
    budget=os.environ.get('HYPERCAST_TEST_UPLOAD_MBPS','25');assert budget in {'12','25','40'}
    env.update(HYPERCAST_URL='wss://hypercast.cloudlane.com.br',HYPERCAST_AUTO_CONNECT='1',HYPERCAST_CODEC='h265',HYPERCAST_SMOKE_UPLOAD_MBPS=budget)
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
        assert f' encoder: {expected}' in publisher,'Rust did not select the expected GPU encoder'
        assert 'encoder_window' in publisher and 'applied_bps=' in publisher,'Missing encoder quality measurements'
        assert 'video_rtp' in publisher,'Missing RTP quality counters'
        assert f'bandwidth: video_limit_bps={int(budget)*1_000_000}' in publisher,'Selected video budget was not applied'
        applied=list(map(int,re.findall(r'applied_bps=(\d+)',publisher)))
        assert applied and max(applied)<=int(budget)*1_000_000,'Encoder exceeded selected bitrate target'
        for file in files:file.close()
        viewer=(ROOT/'.tools/amf-window-viewer.log').read_text(encoding='utf-8')
        match=re.search(r'GUI smoke: (\d+) received, (\d+) GPU presentations',viewer)
        assert match and int(match[1])>10 and int(match[2])>10,'AMD stream did not decode/present'
        assert 'connected=true' in viewer
        assert 'GUI shutdown: 0 retained GPU decoder surfaces' in viewer,'GPU decoder surfaces survived shutdown'
        rates=re.findall(r'Receive throughput: ([0-9.]+)',viewer)
        assert rates and max(map(float,rates))>5,'Receiver throughput indicator did not measure live frames'
        print(f'PASS: synthetic WGC -> Rust {expected} HEVC -> SFU -> GPU desktop presentation')
    finally:
        for child in children:
            if child.poll() is None:child.terminate();child.wait(timeout=10)
        for file in files:file.close()
        cleanup(room,key,secret)

if __name__=='__main__':main()
