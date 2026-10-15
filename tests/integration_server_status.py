"""Owned Windows GUI, controlled HTTP health fixture; never joins a room."""
import os
from pathlib import Path
import subprocess
import threading
import time
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer

ROOT = Path(__file__).resolve().parent.parent


def main():
    fixture = {"healthy": True, "requests": 0}

    class Handler(BaseHTTPRequestHandler):
        def log_message(self, *args):
            pass

        def do_GET(self):
            assert self.path == "/"
            assert not self.headers.get("Authorization") and not self.headers.get("Cookie")
            fixture["requests"] += 1
            body = b"OK" if fixture["healthy"] else b"Not Ready"
            self.send_response(200 if fixture["healthy"] else 503)
            self.send_header("Content-Length", str(len(body)))
            self.end_headers()
            self.wfile.write(body)

    server = ThreadingHTTPServer(("127.0.0.1", 0), Handler)
    threading.Thread(target=server.serve_forever, daemon=True).start()
    exe = Path(os.environ.get("HYPERCAST_WINDOWS_EXE", str(ROOT / "target/x86_64-pc-windows-msvc/debug/hypercast.exe")))
    env = {k: v for k, v in os.environ.items() if not k.startswith(("HYPERCAST_", "LIVEKIT_"))}
    env.update(HYPERCAST_URL=f"ws://127.0.0.1:{server.server_port}", HYPERCAST_SMOKE_SECONDS="65")
    with (ROOT / ".tools/server-status-gui.log").open("wb") as log:
        app = subprocess.Popen([str(exe), "--gui"], env=env, stdout=log, stderr=subprocess.STDOUT)
        try:
            def wait_label(label, timeout):
                deadline = time.monotonic() + timeout
                while time.monotonic() < deadline:
                    assert app.poll() is None, "GUI exited during health check"
                    result = subprocess.run(["powershell", "-NoProfile", "-ExecutionPolicy", "Bypass", "-File",
                        str(ROOT / "tests/control_room.ps1"), "-TargetPid", str(app.pid),
                        "-ExpectedExecutable", str(exe), "-Name", label, "-Read"], capture_output=True)
                    if result.returncode == 0:
                        return
                    time.sleep(0.5)
                raise AssertionError(f"Server label did not become {label}")

            wait_label("ONLINE", 12)
            from tests.integration_desktop import screenshot
            screenshot(app.pid, ROOT / ".tools/server-status-online.png")
            # Authentication and room membership are intentionally absent.
            wait_label("Entrar no servidor", 3)
            fixture["healthy"] = False
            wait_label("OFFLINE", 23)
            fixture["healthy"] = True
            wait_label("ONLINE", 23)
            assert fixture["requests"] >= 3
            print("PASS: GUI server online -> offline -> online without credentials or room membership")
        finally:
            if app.poll() is None:
                app.terminate()
            app.wait(timeout=10)
            server.shutdown()
            server.server_close()


if __name__ == "__main__":
    main()
