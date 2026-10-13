"""Local Windows/GPU integration: publisher -> SFU -> headless decoder sink.

Requires the native executable with nvenc,livekit-network and local SFU.
Room tokens stay in child environments; no raw/encoded screen recording is made.
"""
import os
from pathlib import Path
import re
import subprocess
import uuid
from tools import room_token as rt

ROOT = Path(__file__).resolve().parent.parent

def main():
    key, secret = rt.load_env()
    room = "media-test-" + uuid.uuid4().hex[:12]
    exe = Path(os.environ.get("HYPERCAST_TEST_EXE", str(ROOT / "target/x86_64-pc-windows-msvc/debug/hypercast.exe")))
    publisher_token = rt.issue(room, "publisher-test", "publisher", key, secret,
        ROOT / "infra/admission.sqlite3")
    viewer_token = rt.issue(room, "viewer-test", "viewer", key, secret,
        ROOT / "infra/admission.sqlite3")
    publisher_env = {**os.environ, "HYPERCAST_ROOM_TOKEN": publisher_token}
    viewer_env = {**os.environ, "HYPERCAST_ROOM_TOKEN": viewer_token}
    codec = os.environ.get("HYPERCAST_TEST_CODEC", "av1")
    log_path = ROOT / ".tools" / (room + "-publisher.log")
    log_path.parent.mkdir(parents=True, exist_ok=True)
    publisher_log = log_path.open("wb")
    publisher = subprocess.Popen([str(exe), "--encode", "--codec", codec,
        "--publish-url", "ws://127.0.0.1:8100", "--seconds", "18"],
        env=publisher_env, cwd=ROOT, stdout=publisher_log, stderr=subprocess.STDOUT)
    try:
        viewer = subprocess.run([str(exe), "--receive-url", "ws://127.0.0.1:8100",
            "--view-mode", "spotlight", "--focus", "hc-screen-1",
            "--pause-after", "6", "--seconds", "12"],
            env=viewer_env, cwd=ROOT, capture_output=True, text=True,
            encoding="utf-8", errors="replace", timeout=35)
        publisher.wait(timeout=35)
        publisher_log.flush()
        publisher_output = log_path.read_text(encoding="utf-8", errors="replace")
        print(viewer.stdout)
        print("\n".join(line for line in publisher_output.splitlines() if not line.startswith("Codec diagnostic:")))
        assert viewer.returncode == 0, viewer.stderr
        assert publisher.returncode == 0, "Publisher failed"
        result = re.search(r"Receiver total: (\d+) decoded frames", viewer.stdout)
        assert result and int(result.group(1)) >= 10, "Insufficient decoded frames"
        gpu = re.search(r"Receiver GPU: (\d+) D3D11 texture frames", viewer.stdout)
        assert gpu and int(gpu.group(1)) == int(result.group(1)), "Decode escaped the D3D11VA surface path"
        paused = re.search(r"unsubscribing all screen tracks at (\d+) frames", viewer.stdout)
        assert paused, "Pause was not exercised"
        assert int(result.group(1)) - int(paused.group(1)) <= 3, "Decoder kept consuming after pause"
        packets = re.search(r"WebRTC outbound: (\d+) packets", publisher_output)
        assert packets and int(packets.group(1)) > 0, "No RTP sent"
        print(f"PASS: native {codec} publishing, selective subscription, GPU decoded frames, pause/unsubscribe")
    finally:
        if publisher.poll() is None:
            publisher.terminate()
            publisher.wait(timeout=10)
        publisher_log.close()

if __name__ == "__main__":
    main()
