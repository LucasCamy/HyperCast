"""Probe server-side publication rejection using valid JWTs, without screen capture."""
import os
import subprocess
import time
import uuid
from pathlib import Path
from tools import room_token as rt

ROOT = Path(__file__).resolve().parent.parent

def main():
    key, secret = rt.load_env()
    room = "quota-test-" + uuid.uuid4().hex[:12]
    exe = ROOT / "target/x86_64-pc-windows-msvc/debug/hypercast.exe"
    for identity, forbidden in [("hc-screen-1", False), ("hc-screen-4", True), ("unassigned", True)]:
        now = int(time.time())
        token = rt.sign({"iss": key, "sub": identity, "nbf": now-5, "exp": now+120,
            "video": {"room": room, "roomJoin": True, "canPublish": True,
                      "canSubscribe": True, "canPublishSources": ["screen_share"]}}, secret)
        args = [str(exe), "--admission-probe-url", "ws://127.0.0.1:8100"]
        if forbidden:
            args.append("--reject-first")
        result = subprocess.run(args, cwd=ROOT, env={**os.environ, "HYPERCAST_ROOM_TOKEN": token},
            capture_output=True, text=True, encoding="utf-8", errors="replace", timeout=45)
        assert result.returncode == 0, (result.stdout + result.stderr).replace(token, "[token]")
        assert "PASS: SFU rejected" in result.stdout
        print(identity, result.stdout.strip())
    print("PASS: genuine SFU admission; duplicate and out-of-pool identities rejected")

if __name__ == "__main__":
    main()
