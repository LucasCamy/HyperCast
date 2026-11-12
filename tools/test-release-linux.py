"""Run the packaged AppImage in clean and upgrade profiles on a headless Linux runner."""

import hashlib
import json
import os
from pathlib import Path
import shutil
import subprocess
import sys
import tempfile


def verify(package):
    checksum = hashlib.sha256(package.read_bytes()).hexdigest()
    sums = package.with_name("SHA256SUMS-linux.txt").read_text().splitlines()
    if f"{checksum}  {package.name}" not in sums:
        raise RuntimeError(f"Checksum mismatch for {package.name}")


def profile_environment(profile):
    environment = dict(os.environ)
    for variable in ("DISPLAY", "WAYLAND_DISPLAY", "WAYLAND_SOCKET", "APPIMAGE_EXTRACT_AND_RUN"):
        environment.pop(variable, None)
    for variable in ("XDG_STATE_HOME", "XDG_CONFIG_HOME", "XDG_CACHE_HOME", "XDG_DATA_HOME", "XDG_RUNTIME_DIR"):
        path = profile / variable.lower()
        path.mkdir(parents=True, mode=0o700)
        environment[variable] = str(path)
    environment.update(HYPERCAST_AUTO_CONNECT="0", HYPERCAST_URL="ws://127.0.0.1:9",
                       HYPERCAST_SMOKE_SECONDS="5", XDG_SESSION_TYPE="x11")
    return environment


def launch(package, environment):
    subprocess.run(["xvfb-run", "-a", str(package), "--appimage-extract-and-run", "--gui"],
                   env=environment, check=True, timeout=60)


candidate, baseline = (Path(value).resolve() for value in sys.argv[1:])
for artifact in (candidate, baseline):
    verify(artifact)
candidate.chmod(0o755)
with tempfile.TemporaryDirectory(prefix="hypercast-appimage-test-") as directory:
    root = Path(directory)
    clean = profile_environment(root / "clean")
    launch(candidate, clean)
    subprocess.run([str(candidate), "--appimage-extract-and-run", "--linux-media-test"],
                   env=clean, check=True, timeout=120)
    print("PASS: clean AppImage launch and packaged HEVC media", flush=True)

    upgrade = profile_environment(root / "upgrade")
    state = Path(upgrade["XDG_STATE_HOME"]) / "hypercast"
    state.mkdir()
    fixtures = {
        "display-name": "ReleaseQA",
        "language": "pt-BR",
        "preferences.json": json.dumps({
            "schema": 1, "resolution": "1080p", "fps": 60, "upload_mbps": 12,
            "rate_mode": "manual", "volume": 37, "muted": False, "sidebar": False,
            "self_preview": True, "grid": True,
        }),
    }
    for name, contents in fixtures.items():
        (state / name).write_text(contents, encoding="utf-8")
    installed = root / "HyperCast.AppImage"
    shutil.copyfile(baseline, installed)
    installed.chmod(0o755)
    launch(installed, upgrade)
    shutil.copyfile(candidate, installed)
    launch(installed, upgrade)
    for name, contents in fixtures.items():
        if (state / name).read_text(encoding="utf-8") != contents:
            raise RuntimeError(f"Upgrade changed {name}")
    print("PASS: AppImage upgrade with saved profile")
