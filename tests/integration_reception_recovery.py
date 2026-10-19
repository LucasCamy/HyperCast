"""Isolated SFU test with two generated screen/audio sources; no desktop capture."""
import json, os, shutil, subprocess, tempfile, time, tomllib, urllib.error, urllib.request, uuid
from pathlib import Path
from tools import room_token as rt
ROOT = Path(__file__).resolve().parent.parent

def main():
    windows = os.environ.get('HYPERCAST_RECOVERY_WINDOWS') == '1'
    distro = os.environ.get('HYPERCAST_WSL_DISTRO', 'Ubuntu-24.04')
    binary = None
    if not windows:
        artifacts = [json.loads(line) for line in (ROOT/'.tools/pipewire-frame-build.jsonl').read_text().splitlines() if line.startswith('{')]
        binary = next(a['executable'] for a in artifacts if a.get('reason') == 'compiler-artifact' and a.get('target', {}).get('name') == 'hypercast' and a.get('executable'))
        assert binary.startswith('/home/lucas/hypercast-target/debug/deps/hypercast-')
    version = tomllib.loads((ROOT/'Cargo.toml').read_text(encoding='utf-8'))['package']['version']
    image = '/mnt/f/HyperCast/dist/HyperCast-'+version+'-linux-x86_64.AppImage'
    key, secret = rt.load_env(ROOT/'.tools/homelab.env')
    room = 'recovery-test-'+uuid.uuid4().hex[:12]
    now = int(time.time())
    def token(publish, slot):
        grant = {'room':room, 'roomJoin':True, 'canPublish':publish, 'canSubscribe':True, 'canPublishData':False}
        if publish: grant['canPublishSources'] = ['screen_share', 'screen_share_audio']
        return rt.sign({'iss':key, 'sub':('hc-screen-'+str(slot)) if publish else 'hc-test-'+uuid.uuid4().hex, 'nbf':now-5, 'exp':now+240, 'video':grant}, secret)
    base = {k:v for k,v in os.environ.items() if not k.startswith(('HYPERCAST_', 'LIVEKIT_', 'GH_', 'GITHUB_'))}
    children, logs = [], []
    stage = None
    def launch(name, command, publish, slot=0):
        log = (ROOT/('.tools/recovery-'+('windows' if windows else 'linux')+'-'+name+'.log')).open('wb'); logs.append(log)
        env = {**base, 'HYPERCAST_URL':'wss://hypercast.cloudlane.com.br', 'HYPERCAST_ROOM_TOKEN':token(publish, slot), 'HYPERCAST_AUDIO_TEST_SILENT_OUTPUT':'1', 'HYPERCAST_SWITCH_CASE':'automatic_reception_recovers_only_stalled_stream'}
        env['WSLENV'] = ':'.join(k+'/u' for k in env if k.startswith('HYPERCAST_'))
        env['PATH'] = str(ROOT/'.tools/ffmpeg/bin')+os.pathsep+env['PATH']
        child = subprocess.Popen(command, env=env, stdout=log, stderr=subprocess.STDOUT); children.append(child); return child
    try:
        for slot in (1,2):
            launch('publisher'+str(slot), ['wsl','-d',distro,'--',image,'--appimage-extract-and-run','--linux-publish-test-url','wss://hypercast.cloudlane.com.br','--seconds','110'], True, slot)
        if windows:
            executable = max((ROOT/'target/x86_64-pc-windows-msvc/debug/deps').glob('hypercast-*.exe'),key=lambda p:p.stat().st_mtime)
            stage = tempfile.TemporaryDirectory(prefix='recovery-native-', dir=ROOT/'.tools')
            staged = Path(stage.name)/'hypercast-test.exe'
            shutil.copy2(executable, staged)
            # Native FFmpeg loads pinned DLLs beside its executable, never from PATH.
            for dll in (ROOT/'.tools/ffmpeg/bin').glob('*.dll'): shutil.copy2(dll, Path(stage.name)/dll.name)
            command = [str(staged),'automatic_reception_recovers_only_stalled_stream','--ignored','--nocapture']
        else:
            command = ['wsl','-d',distro,'--','python3','-m','tests.integration_linux_publisher_switch','--linux-probe',binary]
        viewer = launch('viewer',command,False)
        result_code = viewer.wait(timeout=100)
        assert result_code == 0, f'Recovery test failed ({result_code}); inspect scoped viewer log'
        for log in logs: log.flush()
        result = (ROOT/('.tools/recovery-'+('windows' if windows else 'linux')+'-viewer.log')).read_bytes()
        assert b'test result: ok. 1 passed' in result, 'Test filter did not execute exactly one workload'
        print('PASS: '+('Windows' if windows else 'Linux')+' automatic recovery with two generated video/audio sources')
    finally:
        for child in children:
            if child.poll() is None: child.terminate(); child.wait(timeout=10)
        for log in logs: log.close()
        if stage is not None: stage.cleanup()
        admin = rt.sign({'iss':key,'sub':'operator','nbf':now-5,'exp':now+240,'video':{'room':room,'roomAdmin':True,'roomCreate':True}},secret)
        request = urllib.request.Request('https://hypercast.cloudlane.com.br/twirp/livekit.RoomService/DeleteRoom',data=json.dumps({'room':room}).encode(),headers={'Content-Type':'application/json','Authorization':'Bearer '+admin,'User-Agent':'HyperCast-Test'})
        try:
            with urllib.request.urlopen(request, timeout=10): pass
        except urllib.error.HTTPError as error:
            if error.code != 404: raise

if __name__ == '__main__': main()
