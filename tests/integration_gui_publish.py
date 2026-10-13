"""Native UI publishes, stops, republishes and previews on the same room session."""
import os
from pathlib import Path
import re
import subprocess
import time
import uuid
from tools import room_token as rt
from tests.integration_desktop import screenshot

ROOT=Path(__file__).resolve().parent.parent

def main():
    key,secret=rt.load_env()
    room="gui-pub-"+uuid.uuid4().hex[:12]
    exe=ROOT/"target/x86_64-pc-windows-msvc/debug/hypercast.exe"
    publisher_token=rt.issue(room,"Lucas","publisher",key,secret,ROOT/"infra/admission.sqlite3")
    viewer_token=rt.issue(room,"Viewer","viewer",key,secret,ROOT/"infra/admission.sqlite3")
    gui=subprocess.Popen([str(exe),"--gui"],cwd=ROOT,stdout=subprocess.PIPE,stderr=subprocess.STDOUT,
        env={**os.environ,"HYPERCAST_ROOM_TOKEN":publisher_token,"HYPERCAST_AUTO_CONNECT":"1","HYPERCAST_SMOKE_SHARE":"1","HYPERCAST_SMOKE_SECONDS":"14"})
    viewer=None
    try:
        viewer=subprocess.Popen([str(exe),"--receive-url","ws://127.0.0.1:8100","--seconds","16"],cwd=ROOT,
            stdout=subprocess.PIPE,stderr=subprocess.STDOUT,env={**os.environ,"HYPERCAST_ROOM_TOKEN":viewer_token})
        time.sleep(11)
        screenshot(gui.pid,ROOT/".tools/layout-publishing.png")
        raw,_=gui.communicate(timeout=35)
        output=raw.decode("utf-8",errors="replace")
        raw,_=viewer.communicate(timeout=35)
        received=raw.decode("utf-8",errors="replace")
        print(output.replace(publisher_token,"[token]")); print(received.replace(viewer_token,"[token]"))
        assert gui.returncode == 0 and viewer.returncode == 0
        assert output.count("Published screen track") == 2,"Stop/republish failed"
        assert "GUI publisher: stopped" in output
        smoke=re.search(r"GUI smoke: \d+ received, (\d+) GPU presentations; error: (.*)",output)
        assert smoke and int(smoke.group(1)) > 10 and not smoke.group(2).strip()
        count=re.search(r"Receiver GPU: (\d+) D3D11 texture frames",received)
        assert count and int(count.group(1)) > 100
        print("PASS: GUI NVENC publication, immutable GPU preview, stop and republish")
    finally:
        for process in (gui,viewer):
            if process and process.poll() is None:
                process.terminate(); process.wait(timeout=10)

if __name__ == "__main__":
    main()
