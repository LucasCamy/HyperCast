"""Read only the owned GUI; language comes from the installer's preference file."""
import os,subprocess,time
from pathlib import Path
from tests.integration_desktop import screenshot
ROOT=Path(__file__).resolve().parent.parent

def main():
    binary=Path(os.environ['HYPERCAST_TEST_EXE']).resolve()
    env={k:v for k,v in os.environ.items() if not k.startswith(('HYPERCAST_','LIVEKIT_'))}
    env.update(HYPERCAST_SMOKE_SECONDS='18',HYPERCAST_NAME='LanguageTester')
    with (ROOT/'.tools/language-ui.log').open('wb') as log:
        app=subprocess.Popen([str(binary),'--gui'],env=env,stdout=log,stderr=subprocess.STDOUT)
        try:
            deadline=time.monotonic()+8
            while True:
                check=subprocess.run(['powershell','-NoProfile','-ExecutionPolicy','Bypass','-File',str(ROOT/'tests/control_room.ps1'),'-TargetPid',str(app.pid),'-ExpectedExecutable',str(binary),'-Name','Ready to connect','-Read'],capture_output=True,timeout=5)
                if check.returncode==0:break
                assert time.monotonic()<deadline,'Installed GUI did not use the persisted English language'
                time.sleep(.2)
            subprocess.run(['powershell','-NoProfile','-ExecutionPolicy','Bypass','-File',str(ROOT/'tests/control_room.ps1'),'-TargetPid',str(app.pid),'-ExpectedExecutable',str(binary),'-Name','Video format · HEVC'],capture_output=True,timeout=5)
            time.sleep(.3)
            for name in ['Resolution','Send FPS']:
                translated=subprocess.run(['powershell','-NoProfile','-ExecutionPolicy','Bypass','-File',str(ROOT/'tests/control_room.ps1'),'-TargetPid',str(app.pid),'-ExpectedExecutable',str(binary),'-Name',name,'-Read'],capture_output=True,timeout=5)
                assert translated.returncode==0,'Untranslated stream setting: '+name
            screenshot(app.pid,ROOT/'.tools/language-english.png')
            app.wait(timeout=22)
            assert app.returncode==0,'English GUI failed during shutdown'
            print('PASS: installed application reused English without a language environment override')
        finally:
            if app.poll() is None:app.terminate();app.wait(timeout=10)

if __name__=='__main__':main()
