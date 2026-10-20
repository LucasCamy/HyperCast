"""Windows GUI/WGC/NVENC/SFU cadence regression using only a generated app window."""
import json
import os
import re
import subprocess
import time
import tomllib
import urllib.error
import urllib.request
import uuid
from pathlib import Path
from tools import room_token as rt

ROOT = Path(__file__).resolve().parent.parent


def main():
    version = tomllib.loads((ROOT / 'Cargo.toml').read_text(encoding='utf-8'))['package']['version']
    executable = ROOT / 'dist' / f'HyperCast-{version}-windows-x64/hypercast.exe'
    room = 'cadence-test-' + uuid.uuid4().hex[:12]
    key, secret = rt.load_env(ROOT / '.tools/homelab.env')
    now = int(time.time())

    def token(publish):
        grant = dict(room=room, roomJoin=True, canPublish=publish, canSubscribe=True, canPublishData=False)
        if publish:
            grant['canPublishSources'] = ['screen_share']
        return rt.sign(dict(iss=key, sub='hc-member-' + uuid.uuid4().hex, nbf=now-5, exp=now+300, video=grant), secret)

    base = {k: v for k, v in os.environ.items() if not k.startswith(('HYPERCAST_', 'LIVEKIT_'))}
    profile = ROOT / '.tools' / ('cadence-profile-' + uuid.uuid4().hex)
    data = profile / 'HyperCast'
    data.mkdir(parents=True)
    children, logs = [], []

    def launch(label, args, env):
        log = (ROOT / '.tools' / ('cadence-' + label + '.log')).open('wb')
        logs.append(log)
        child = subprocess.Popen(args, env=env, stdout=log, stderr=subprocess.STDOUT)
        children.append(child)
        return child

    try:
        source = launch('source', [str(ROOT / '.tools/ffmpeg/bin/ffplay.exe'), '-window_title', 'HyperCast generated cadence test', '-an', '-f', 'lavfi', '-i', 'testsrc2=size=1280x720:rate=120'], base)
        time.sleep(2)
        assert source.poll() is None, 'Generated window exited'
        viewer = launch('viewer', [str(executable), '--receive-url', 'wss://hypercast.cloudlane.com.br', '--seconds', '105'], {**base, 'HYPERCAST_ROOM_TOKEN': token(False)})
        publisher = launch('publisher', [str(executable), '--gui'], {**base, 'LOCALAPPDATA': str(profile), 'HYPERCAST_URL': 'wss://hypercast.cloudlane.com.br', 'HYPERCAST_ROOM_TOKEN': token(True), 'HYPERCAST_AUTO_CONNECT': '1', 'HYPERCAST_SMOKE_SHARE': '1', 'HYPERCAST_SMOKE_WINDOW_PID': str(source.pid), 'HYPERCAST_SMOKE_SECONDS': '95', 'HYPERCAST_SMOKE_SHARE_CYCLES': '2', 'HYPERCAST_SMOKE_AUTOMATIC': '1', 'HYPERCAST_SYSTEM_AUDIO': '0'})
        assert publisher.wait(timeout=120) == 0, 'Publisher failed'
        assert viewer.wait(timeout=25) == 0, 'Viewer failed'
        for log in logs:
            log.flush()
        diagnostic = next((data / 'diagnostics').glob(f'gui-*-{publisher.pid}.log')).read_text(encoding='utf-8')
        output = (ROOT / '.tools/cadence-publisher.log').read_text(encoding='utf-8', errors='replace')
        report = json.loads(next(line.removeprefix('GUI technical report: ') for line in output.splitlines() if line.startswith('GUI technical report: ')))
        assert report['version'] == version and report['adaptive_upload']['mode'] == 'automatic'
        assert report['requested']['fps'] == report['video']['accepted_encoder_config']['fps'] == 120
        windows = [line for line in diagnostic.splitlines() if ' encoder_window:' in line]
        assert len(windows) >= 12, 'Insufficient sustained encode evidence'
        assert all('fps=120 ' in line for line in windows), 'Encoder cadence ratcheted down'
        counts = [int(re.search(r'frames=(\d+)', line)[1]) for line in windows[3:]]
        assert min(counts[-3:]) >= counts[0] * .8, 'Progressive send throughput loss'
        assert not any(tag in diagnostic for tag in (' capture_error:', ' publication_error:', ' source_switch:')), 'Unexpected capture failure/source change'
        received = (ROOT / '.tools/cadence-viewer.log').read_text(encoding='utf-8', errors='replace')
        assert int(re.search(r'Receiver GPU: (\d+) D3D11 texture frames', received)[1]) > 1000, 'Missing GPU reception'
        print(f'PASS: {len(windows)} encode windows; configured/applied 120 FPS; early_frames={counts[0]} late_frames={counts[-3:]}; generated-window GUI publication and GPU reception')
    finally:
        for child in children:
            if child.poll() is None:
                child.terminate()
                child.wait(timeout=10)
        for log in logs:
            log.close()
        admin = rt.sign(dict(iss=key, sub='cadence-cleanup', nbf=now-5, exp=now+300, video=dict(room=room, roomAdmin=True, roomCreate=True)), secret)
        request = urllib.request.Request('https://hypercast.cloudlane.com.br/twirp/livekit.RoomService/DeleteRoom', data=json.dumps(dict(room=room)).encode(), headers={'Content-Type': 'application/json', 'Authorization': 'Bearer ' + admin, 'User-Agent': 'HyperCast-Test'})
        try:
            with urllib.request.urlopen(request, timeout=10):
                pass
        except urllib.error.HTTPError as error:
            if error.code != 404:
                raise


if __name__ == '__main__':
    main()
