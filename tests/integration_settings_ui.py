"""Exercise persisted choices in an isolated Windows profile; capture only the owned UI."""
import ctypes,json,os,subprocess,tempfile,time
from pathlib import Path
from tests.integration_desktop import owned_window,screenshot
ROOT=Path(__file__).resolve().parent.parent

def main():
    binary=Path(os.environ['HYPERCAST_TEST_EXE']).resolve()
    with tempfile.TemporaryDirectory(prefix='hypercast-settings-ui-') as folder:
        data=Path(folder)/'HyperCast';data.mkdir()
        initial=dict(schema=1,resolution='1080p',fps=60,upload_mbps=12,volume=35,muted=False,sidebar=True,self_preview=False,grid=True)
        pref=data/'preferences.json';pref.write_text(json.dumps(initial),encoding='utf-8')
        env={k:v for k,v in os.environ.items() if not k.startswith(('HYPERCAST_','LIVEKIT_'))}
        env.update(LOCALAPPDATA=folder,HYPERCAST_NAME='SettingsTester',HYPERCAST_LANGUAGE='en-US')
        def control(pid,name,read=False):
            args=['powershell','-NoProfile','-ExecutionPolicy','Bypass','-File',str(ROOT/'tests/control_room.ps1'),'-TargetPid',str(pid),'-ExpectedExecutable',str(binary),'-Name',name]
            if read:args.append('-Read')
            return subprocess.run(args,capture_output=True,timeout=8).returncode
        for run in range(2):
            with (ROOT/f'.tools/settings-ui-{run}.log').open('wb') as log:
                app=subprocess.Popen([str(binary),'--gui'],env=env,stdout=log,stderr=subprocess.STDOUT)
                try:
                    until=time.monotonic()+12
                    while control(app.pid,'Ready to connect' if run==0 else 'Open menu',True):
                        assert time.monotonic()<until,'Settings GUI did not become ready';time.sleep(.2)
                    if run==1:assert control(app.pid,'Open menu')==0
                    assert control(app.pid,'Video format · HEVC')==0
                    time.sleep(.3)
                    screenshot(app.pid,ROOT/'.tools/settings-selections-0.9.0.png')
                    for name in ['Resolution','Send FPS','Received audio · 35%']:
                        assert control(app.pid,name,True)==0,'Persisted selection missing: '+name
                    if run==0:
                        assert control(app.pid,'Debug')==0;time.sleep(.3)
                        assert control(app.pid,'Copy technical report',True)==0
                        screenshot(app.pid,ROOT/'.tools/settings-diagnostic-0.9.0.png')
                    # Collapse is persisted on normal exit; does not connect or capture.
                    assert control(app.pid,'Collapse menu')==0;time.sleep(.3)
                    user,hwnd=owned_window(app.pid)
                    user.PostMessageW.argtypes=[ctypes.c_void_p,ctypes.c_uint,ctypes.c_size_t,ctypes.c_ssize_t]
                    user.PostMessageW(hwnd,0x0010,0,0);app.wait(timeout=12)
                    assert app.returncode==0,'Settings GUI failed to close cleanly'
                    saved=json.loads(pref.read_text(encoding='utf-8'))
                    assert saved['sidebar'] is False
                    for key in ['resolution','fps','upload_mbps','volume','self_preview']:assert saved[key]==initial[key],key
                finally:
                    if app.poll() is None:app.terminate();app.wait(timeout=10)
        print('PASS: persisted resolution, FPS, profile, volume, preview and menu; technical report control; clean restart')
if __name__=='__main__':main()
