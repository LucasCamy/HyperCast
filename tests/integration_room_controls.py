"""Owned generated windows/streams only; isolated room removed after the UI test.

Checks local preview, the participants panel, stage selection and full screen.
"""
import json, os, re, subprocess, time, urllib.request, uuid
from pathlib import Path
from tools import room_token as rt
from tests.integration_desktop import screenshot
ROOT=Path(__file__).resolve().parent.parent

def main():
    key,secret=rt.load_env(ROOT/'.tools/homelab.env');room='controls-'+uuid.uuid4().hex[:10];now=int(time.time())
    def token(identity,name):
        return rt.sign({'iss':key,'sub':{'control-a':'hc-screen-1','control-b':'hc-screen-2','control-viewer':'hc-screen-3'}[identity],'name':name,'nbf':now-5,'exp':now+180,'video':{'room':room,'roomJoin':True,'canPublish':True,'canSubscribe':True,'canPublishData':False,'canPublishSources':['screen_share','screen_share_audio']}},secret)
    base={k:v for k,v in os.environ.items() if not k.startswith(('HYPERCAST_','LIVEKIT_','GH_','GITHUB_'))}
    exe=Path(os.environ.get('HYPERCAST_WINDOWS_EXE',str(ROOT/'dist/HyperCast-0.7.2-windows-x64/hypercast.exe')))
    linux=os.environ.get('HYPERCAST_LINUX_EXE','/home/lucas/hypercast-target/release/hypercast')
    children=[];logs=[]
    def control(name='',*flags):
        result=subprocess.run(['powershell','-NoProfile','-ExecutionPolicy','Bypass','-File',str(ROOT/'tests/control_room.ps1'),'-TargetPid',str(gui.pid),'-ExpectedExecutable',str(exe),'-Name',name,*flags],stdout=subprocess.PIPE,stderr=subprocess.PIPE,timeout=12)
        assert result.returncode==0, result.stderr.decode(errors='replace')
        return result.stdout.decode('utf-8',errors='replace')
    def tiles():return control('', '-List').splitlines()
    def on_stage():return [tile for tile in tiles() if tile.startswith('No palco:')]
    try:
        app=subprocess.Popen([str(ROOT/'.tools/ffmpeg/bin/ffplay.exe'),'-window_title','HyperCast controls generated test','-x','640','-y','360','-f','lavfi','-i','testsrc2=size=640x360:rate=60'],stdout=subprocess.DEVNULL,stderr=subprocess.DEVNULL);children.append(app);time.sleep(2)
        log=(ROOT/'.tools/room-controls-gui.log').open('wb');logs.append(log)
        env={**base,'HYPERCAST_NAME':'Controls','HYPERCAST_URL':'wss://hypercast.cloudlane.com.br','HYPERCAST_ROOM_TOKEN':token('control-viewer','Controls'),'HYPERCAST_AUTO_CONNECT':'1','HYPERCAST_SMOKE_SECONDS':'55','HYPERCAST_SMOKE_SHARE':'1','HYPERCAST_SMOKE_WINDOW_PID':str(app.pid),'HYPERCAST_VIEW_MODE':'grid','HYPERCAST_CODEC':'hevc'}
        gui=subprocess.Popen([str(exe),'--gui'],env=env,stdout=log,stderr=subprocess.STDOUT);children.append(gui)
        for identity,name in [('control-a','Alpha'),('control-b','Beta')]:
            log=(ROOT/f'.tools/room-controls-{name}.log').open('wb');logs.append(log)
            args=[linux,*(['--appimage-extract-and-run'] if linux.endswith('.AppImage') else []),'--linux-publish-test-url','wss://hypercast.cloudlane.com.br','--seconds','50']
            child=subprocess.Popen(['wsl','-d','Ubuntu','--',*args],env={**base,'HYPERCAST_ROOM_TOKEN':token(identity,name),'WSLENV':'HYPERCAST_ROOM_TOKEN/u'},stdout=log,stderr=subprocess.STDOUT);children.append(child)
        time.sleep(13)
        assert len(tiles())==3, f'Expected three grid tiles, got {tiles()}'
        screenshot(gui.pid,ROOT/'.tools/room-controls-grid.png')
        control('Ocultar minha prévia');time.sleep(1)
        assert len(tiles())==2,'Self-preview remains visible'
        control('Mostrar minha prévia');time.sleep(1)
        assert len(tiles())==3,'Self-preview did not resume'
        control('Ocultar minha prévia');time.sleep(1)
        control('Ocultar participantes');time.sleep(.5)
        control('Mostrar participantes');time.sleep(.5)
        control('Colocar Alpha · Tela no palco');time.sleep(.5)
        assert on_stage()==['No palco: Alpha · Tela'],f'Alpha was not on stage: {tiles()}'
        control('No palco: Alpha · Tela','-Double');time.sleep(.5)
        control('', '-Fullscreen')
        screenshot(gui.pid,ROOT/'.tools/room-controls-single.png')
        control('No palco: Alpha · Tela','-Double');time.sleep(.5)
        control('Beta: Compartilhando tela');time.sleep(.5)
        assert on_stage()==['No palco: Beta · Tela'],'Participant selection did not focus Beta'
        try:
            control('Grade')
        except AssertionError:
            control('Visualização · Tudo')
            control('Grade')
        time.sleep(.5)
        assert len(tiles())==2 and not on_stage(),'Grid did not restore both streams'
        control('Ocultar participantes');time.sleep(.5)
        screenshot(gui.pid,ROOT/'.tools/room-controls-wide.png')
        assert gui.wait(timeout=35)==0,'Owned GUI failed'
        for log in logs:log.flush()
        output=(ROOT/'.tools/room-controls-gui.log').read_text(encoding='utf-8',errors='replace')
        assert int(re.search(r'GUI publisher: (\d+) sent',output)[1])>300,'Publication stopped with preview disabled'
        assert 'GUI preview: enabled=false local_surfaces=0' in output,'Preview GPU surface was retained'
        print('PASS: participants panel, preview stop/resume, stage selection, full screen and Grid')
    finally:
        for child in children:
            if child.poll() is None:child.terminate();child.wait(timeout=10)
        for log in logs:log.close()
        admin=rt.sign({'iss':key,'sub':'operator','nbf':now-5,'exp':now+180,'video':{'room':room,'roomAdmin':True,'roomCreate':True}},secret)
        request=urllib.request.Request('https://hypercast.cloudlane.com.br/twirp/livekit.RoomService/DeleteRoom',data=json.dumps({'room':room}).encode(),headers={'Content-Type':'application/json','Authorization':'Bearer '+admin,'User-Agent':'HyperCast-Operator/0.7'})
        try:
            with urllib.request.urlopen(request,timeout=10):pass
        except urllib.error.HTTPError as error:
            if error.code!=404:raise
if __name__=='__main__':main()

