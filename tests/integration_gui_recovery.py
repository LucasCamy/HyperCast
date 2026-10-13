"""Exercise bounded recovery with a debug-only capture failure, real NVENC and SFU.

This verifies reconstruction and publication lifecycle, not physical device loss.
"""
import os
from pathlib import Path
import subprocess
import uuid
from tools import room_token as rt

ROOT = Path(__file__).resolve().parent.parent

def main():
    key, secret = rt.load_env()
    room = 'recovery-' + uuid.uuid4().hex[:12]
    token = rt.issue(room, 'Recovery', 'publisher', key, secret, ROOT/'infra/admission.sqlite3')
    exe = ROOT/'target/x86_64-pc-windows-msvc/debug/hypercast.exe'
    before = set((Path(os.environ['LOCALAPPDATA'])/'HyperCast/diagnostics').glob('gui-*.log'))
    run = subprocess.run([str(exe), '--gui'], cwd=ROOT, stdout=subprocess.PIPE, stderr=subprocess.STDOUT,
        timeout=35, env={**os.environ,"HYPERCAST_URL":os.environ.get("HYPERCAST_TEST_URL","ws://127.0.0.1:8100"), 'HYPERCAST_ROOM_TOKEN': token, 'HYPERCAST_AUTO_CONNECT': '1',
        'HYPERCAST_CODEC': 'h265', 'HYPERCAST_SMOKE_SHARE': '1', 'HYPERCAST_SMOKE_SECONDS': '14',
        'HYPERCAST_SMOKE_CAPTURE_LOSS': '1'})
    output = run.stdout.decode('utf-8', errors='replace')
    print(output.replace(token, '[token]'))
    assert run.returncode == 0
    logs = set((Path(os.environ['LOCALAPPDATA'])/'HyperCast/diagnostics').glob('gui-*.log')) - before
    assert len(logs) == 1
    diagnostic = next(iter(logs)).read_text(encoding='utf-8')
    assert token not in diagnostic and secret not in diagnostic
    assert 'Injected capture loss' in diagnostic
    assert diagnostic.count('Rebuilding GPU pipeline, attempt 1/3') == 1
    assert 'attempt 2/3' not in diagnostic
    assert output.count('Published screen track') == 3
    assert 'connected=true, can_publish=true' in output
    assert 'Compartilhando sua tela' in output
    print('PASS: injected failure rebuilt capture/encoder, republished, and saved redacted GUI log')

if __name__ == '__main__':
    main()
