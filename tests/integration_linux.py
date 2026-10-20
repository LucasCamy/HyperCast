"""Generated HEVC -> SFU -> Linux GUI/FFmpeg and Windows D3D11, no desktop recording.
Runs on the Windows dev host with an owned Ubuntu WSL installation. Credentials
remain in environment variables; the unique test room is removed afterwards.
"""
import json,os,re,subprocess,time,urllib.request,uuid
from pathlib import Path
from tools import room_token as rt
ROOT=Path(__file__).resolve().parent.parent

def main():
    key,secret=rt.load_env(ROOT/'.tools/homelab.env');room='linux-'+uuid.uuid4().hex[:12];now=int(time.time())
    def token(pub):
        grant={'room':room,'roomJoin':True,'canPublish':pub,'canSubscribe':True,'canPublishData':False}
        if pub:grant['canPublishSources']=['screen_share','screen_share_audio']
        return rt.sign({'iss':key,'sub':'hc-member-'+uuid.uuid4().hex,'nbf':now-5,'exp':now+180,'video':grant},secret)
    base={k:v for k,v in os.environ.items() if not k.startswith(('LIVEKIT_','HYPERCAST_','GH_','GITHUB_'))}
    children=[];logs=[]
    binary=os.environ.get('HYPERCAST_LINUX_EXE','/home/lucas/hypercast-target/debug/hypercast')
    gui_cache=ROOT/'.tools'/('linux-gui-cache-'+uuid.uuid4().hex)
    gui_cache_linux='/mnt/'+gui_cache.drive[0].lower()+gui_cache.as_posix()[2:]
    def command(*args):return [binary,*(['--appimage-extract-and-run'] if binary.endswith('.AppImage') else []),*args]
    def launch_linux(name,args,extra=None):
        env={**base,'HYPERCAST_ROOM_TOKEN':token(name=='publisher'),'HYPERCAST_URL':'wss://hypercast.cloudlane.com.br',**(extra or {})}
        names=[n+'/u' for n in env if n.startswith('HYPERCAST_') or n=='XDG_CACHE_HOME'];env['WSLENV']=':'.join(names)
        file=(ROOT/'.tools'/f'linux-{name}.log').open('wb');logs.append(file)
        child=subprocess.Popen(['wsl','-d',os.environ.get('HYPERCAST_WSL_DISTRO','Ubuntu-24.04'),'--',*args],env=env,stdout=file,stderr=subprocess.STDOUT);children.append(child);return child
    try:
        linux=launch_linux('receiver',command('--receive-url','wss://hypercast.cloudlane.com.br','--seconds','60'))
        gui=launch_linux('gui',['xvfb-run','-a',*command('--gui')],{'HYPERCAST_AUTO_CONNECT':'1','HYPERCAST_SMOKE_SECONDS':'60','HYPERCAST_AUDIO_TEST_SILENT_OUTPUT':'1','XDG_CACHE_HOME':gui_cache_linux})
        windows=Path(os.environ.get('HYPERCAST_WINDOWS_EXE',str(ROOT/'dist/HyperCast-0.6.5-windows-x64/hypercast.exe')))
        file=(ROOT/'.tools/linux-windows.log').open('wb');logs.append(file)
        child=subprocess.Popen([str(windows),'--receive-url','wss://hypercast.cloudlane.com.br','--seconds','60'],env={**base,'HYPERCAST_ROOM_TOKEN':token(False)},stdout=file,stderr=subprocess.STDOUT);children.append(child)
        if os.environ.get('HYPERCAST_LINUX_TEST_SIZE'):
            file=(ROOT/'.tools/linux-windows-gui.log').open('wb');logs.append(file)
            viewer=subprocess.Popen([str(windows),'--gui'],env={**base,'HYPERCAST_ROOM_TOKEN':token(False),'HYPERCAST_URL':'wss://hypercast.cloudlane.com.br','HYPERCAST_AUTO_CONNECT':'1','HYPERCAST_SMOKE_SECONDS':'60','HYPERCAST_SMOKE_RECONNECT':'1','HYPERCAST_NAME':'SyntheticViewer'},stdout=file,stderr=subprocess.STDOUT);children.append(viewer)
        # Cold AppImage extraction/WSL startup must not consume the entire
        # Windows diagnostic window before the publisher exists. Wait for all
        # receivers in the scoped room; fixed sleeps hid this timing assumption.
        operator=rt.sign({'iss':key,'sub':'readiness-check','nbf':now-5,'exp':now+180,'video':{'roomList':True}},secret)
        expected=4 if os.environ.get('HYPERCAST_LINUX_TEST_SIZE') else 3
        deadline=time.monotonic()+35
        while True:
            request=urllib.request.Request('https://hypercast.cloudlane.com.br/twirp/livekit.RoomService/ListRooms',data=json.dumps({'names':[room]}).encode(),headers={'Content-Type':'application/json','Authorization':'Bearer '+operator,'User-Agent':'HyperCast-Test'})
            with urllib.request.urlopen(request,timeout=10) as response:rooms=json.load(response).get('rooms',[])
            if any(r.get('num_participants',0)>=expected for r in rooms):break
            assert all(c.poll() is None for c in children),'A receiver exited before publication readiness'
            assert time.monotonic()<deadline,'Receivers did not join the scoped room'
            time.sleep(.3)
        print('All test receivers joined before publication',flush=True)
        time.sleep(3)
        pub=launch_linux('publisher',command('--linux-publish-test-url','wss://hypercast.cloudlane.com.br','--seconds','11'),{n:os.environ[n] for n in ('HYPERCAST_LINUX_TEST_GPU','HYPERCAST_LINUX_TEST_CODEC','HYPERCAST_LINUX_TEST_SIZE','HYPERCAST_LINUX_TEST_UPLOAD_MBPS') if n in os.environ})
        for child in children:
            code=child.wait(timeout=90);assert code==0,f'A media client failed with exit {code}; inspect local logs without credentials'
        for file in logs:file.close()
        if binary.endswith('.AppImage'):
            (ROOT/'.tools/linux-gui.log').write_bytes((gui_cache/'hypercast/hypercast.log').read_bytes())
        read=lambda name:(ROOT/'.tools'/f'linux-{name}.log').read_text(encoding='utf-8',errors='replace')
        if binary.endswith('.AppImage'):
            assert 'HyperCast launcher: exit_code=0' in read('gui'),'GUI launcher did not record a clean exit'
            assert 'HyperCast Linux: version=' in read('gui'),'GUI startup marker missing'
        assert 'Linux capture: stage=first_packet' in read('publisher'),'Capture diagnostic stages missing'
        received=int(re.search(r'Receiver total: (\d+) decoded frames',read('receiver'))[1])
        assert received>100,'Linux did not decode HEVC'
        result=re.search(r'Linux GUI smoke: (\d+) received, (\d+) textures',read('gui'));assert result and int(result[1])>100 and int(result[2])>0,'Linux did not render received video'
        assert int(re.search(r'audio=(\d+)',read('gui'))[1])>100,'Linux did not decode audio'
        def evidence_report(output):
            line=next(line for line in output.splitlines() if line.startswith('GUI technical report: '))
            report=json.loads(line.removeprefix('GUI technical report: '));assert report['end_to_end_latency_ms'] is None
            return report['evidence']['stages']
        linux_stages=evidence_report(read('gui'))
        assert linux_stages['render_prepare_cpu']['samples']>0 and linux_stages['reader_to_render_start']['samples']>0,'Linux GUI timing evidence missing'
        assert linux_stages['present_call'] is None,'Linux texture staging must not claim physical presentation timing'

        windows_log=(ROOT/'.tools/linux-windows.log').read_text(encoding='utf-8',errors='replace')
        assert int(re.search(r'Receiver GPU: (\d+) D3D11 texture frames',windows_log)[1])>100,'Windows did not receive Linux HEVC in GPU'
        assert int(re.search(r'Linux synthetic publisher: (\d+) frames',read('publisher'))[1])>100,'Linux publication failed'
        if os.environ.get('HYPERCAST_LINUX_TEST_UPLOAD_MBPS'):
            budget=int(os.environ['HYPERCAST_LINUX_TEST_UPLOAD_MBPS'])*1_000_000
            if os.environ.get('HYPERCAST_LINUX_TEST_GPU')!='1':budget=min(budget,12_000_000)
            assert f'Linux video budget: {budget} bps' in read('publisher'),'Linux ignored selected upload profile'
        if os.environ.get('HYPERCAST_LINUX_TEST_SIZE'):
            output=(ROOT/'.tools/linux-windows-gui.log').read_text(encoding='utf-8',errors='replace')
            match=re.search(r'GUI smoke: (\d+) received, (\d+) GPU presentations',output)
            assert match and int(match[2])>50,'Windows did not present ultrawide D3D11 frames'
            assert 'GUI lifecycle: 0 retained GPU decoder surfaces after disconnect' in output,'Windows decoder surfaces survived teardown'
            windows_stages=evidence_report(output)
            assert windows_stages['present_call']['samples']>0 and windows_stages['reader_to_render_start']['samples']>0,'Windows presentation timing evidence missing'

        if os.environ.get('HYPERCAST_LINUX_TEST_SIZE')=='4587x1920':
            assert any(size in windows_log for size in ('4586x1920','4588x1920')),'Windows did not decode the nearest even ultrawide size'
            assert any(size in (read('receiver')+read('gui')) for size in ('4586x1920','4588x1920')),'Linux did not decode the nearest even ultrawide size'
        print(f'PASS: Linux {os.environ.get("HYPERCAST_LINUX_TEST_CODEC","hevc").upper()} publishing, Linux decoded reception/audio and GUI textures, Windows GPU reception through SFU')
    finally:
        for child in children:
            if child.poll() is None:child.terminate();child.wait(timeout=10)
        for file in logs:file.close()
        admin=rt.sign({'iss':key,'sub':'operator','nbf':now-5,'exp':now+180,'video':{'room':room,'roomCreate':True,'roomAdmin':True}},secret)
        request=urllib.request.Request('https://hypercast.cloudlane.com.br/twirp/livekit.RoomService/DeleteRoom',data=json.dumps({'room':room}).encode(),headers={'Content-Type':'application/json','Authorization':'Bearer '+admin,'User-Agent':'HyperCast-Operator/0.7'})
        try:
            with urllib.request.urlopen(request,timeout=10):pass
        except urllib.error.HTTPError as error:
            if error.code!=404:raise
if __name__=='__main__':main()
