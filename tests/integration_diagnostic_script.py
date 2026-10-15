"""Windows PowerShell 5.1 regression: pre-token errors and native stderr persist.

Run after packaging. Uses a deliberately invalid token; no screen is published.
"""
import os
from pathlib import Path
import shutil
import subprocess
import uuid

ROOT = Path(__file__).resolve().parent.parent
POWERSHELL = Path(os.environ["SystemRoot"]) / "System32/WindowsPowerShell/v1.0/powershell.exe"


def run(script, local_data):
    local_data.mkdir(parents=True)
    result = subprocess.run([str(POWERSHELL), "-NoProfile", "-File", str(script),
                             "-Role", "Viewer", "-UseEnvironmentToken", "-Seconds", "10"],
        env={**os.environ, "LOCALAPPDATA": str(local_data),
             "HYPERCAST_ROOM_TOKEN": "DIAGNOSTIC_TEST_TOKEN_NOT_REAL"},
        capture_output=True, timeout=85)
    assert result.returncode != 0, "Failure must retain a nonzero exit status"
    errors = list(local_data.glob("HyperCast/diagnostics/*/error.log"))
    assert len(errors) == 1, "Failure before or after the token must save error.log"
    return errors[0].parent


def main():
    fixture = ROOT / ".tools" / ("diagnostic-script-" + uuid.uuid4().hex)
    missing = fixture / "incomplete-package"
    missing.mkdir(parents=True)
    shutil.copyfile(ROOT / "tools/Test-Streaming.ps1", missing / "Test-Streaming.ps1")
    logs = run(missing / "Test-Streaming.ps1", fixture / "missing-data")
    assert "Extraia TODO" in (logs / "error.log").read_text(encoding="utf-8-sig")
    stage = ROOT / "dist/HyperCast-0.7.3-windows-x64"
    logs = run(stage / "Test-Streaming.ps1", fixture / "native-error-data")
    stream = (logs / "stream.log").read_text(encoding="utf-8-sig")
    assert stream.strip(), "Native stderr must be drained and saved under PowerShell 5.1"
    assert "DIAGNOSTIC_TEST_TOKEN_NOT_REAL" not in stream
    assert "DIAGNOSTIC_TEST_TOKEN_NOT_REAL" not in (logs / "error.log").read_text(encoding="utf-8-sig")
    print("PASS: incomplete package explains extraction before token; native errors persist and token is redacted on PowerShell 5.1")


if __name__ == "__main__":
    main()
