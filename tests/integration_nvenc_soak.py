"""Opt-in sustained NVENC test: generated GPU frames only, no room or desktop capture."""
import shutil
import subprocess
import tempfile
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent


def main():
    executable = max((ROOT / 'target/x86_64-pc-windows-msvc/debug/deps').glob('hypercast-*.exe'), key=lambda p: p.stat().st_mtime)
    with tempfile.TemporaryDirectory(prefix='nvenc-soak-', dir=ROOT / '.tools') as directory:
        stage = Path(directory).resolve()
        assert stage.is_relative_to((ROOT / '.tools').resolve())
        binary = stage / 'hypercast-test.exe'
        shutil.copy2(executable, binary)
        for dll in (ROOT / '.tools/ffmpeg/bin').glob('*.dll'):
            shutil.copy2(dll, stage / dll.name)
        with (ROOT / '.tools/nvenc-soak.log').open('wb') as log:
            result = subprocess.run([str(binary), '--exact', 'encoder::ultrawide_tests::sustained_nvenc_cadence_with_bitrate_changes', '--ignored', '--nocapture'], stdout=log, stderr=subprocess.STDOUT, timeout=115)
        assert result.returncode == 0, 'Inspect .tools/nvenc-soak.log'
        output = (ROOT / '.tools/nvenc-soak.log').read_text(encoding='utf-8', errors='replace')
        assert 'test result: ok. 1 passed' in output, 'Native soak did not run'
        print(output)


if __name__ == '__main__':
    main()
