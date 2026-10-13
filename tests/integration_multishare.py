"""Three independent native publishers, GPU Grid/Spotlight/pause/resume viewer."""
import os
from pathlib import Path
import re
import subprocess
import time
import uuid
from tools import room_token as rt
from tests.integration_desktop import screenshot

ROOT = Path(__file__).resolve().parent.parent

def main():
    key, secret = rt.load_env()
    room = "multi-test-" + uuid.uuid4().hex[:12]
    exe = ROOT/"target/x86_64-pc-windows-msvc/debug/hypercast.exe"
    publishers = []
    gui = None
    logs = []
    try:
        for index in range(3):
            token = rt.issue(room,f"Amigo{index+1}","publisher",key,secret,ROOT/"infra/admission.sqlite3")
            log = open(ROOT/f".tools/multi-publisher-{index+1}.log","wb")
            logs.append(log)
            publishers.append(subprocess.Popen([str(exe),"--encode","--codec","av1","--publish-url","ws://127.0.0.1:8100","--seconds","25"],
                cwd=ROOT,env={**os.environ,"HYPERCAST_ROOM_TOKEN":token},stdout=log,stderr=subprocess.STDOUT))
        try:
            rt.issue(room,"Fourth","publisher",key,secret,ROOT/"infra/admission.sqlite3")
        except RuntimeError:
            print("Fourth publisher token rejected")
        else:
            raise AssertionError("Fourth publisher admitted")
        token = rt.issue(room,"Viewer","viewer",key,secret,ROOT/"infra/admission.sqlite3")
        gui = subprocess.Popen([str(exe),"--gui"],cwd=ROOT,stdout=subprocess.PIPE,stderr=subprocess.STDOUT,
            env={**os.environ,"HYPERCAST_ROOM_TOKEN":token,"HYPERCAST_AUTO_CONNECT":"1","HYPERCAST_VIEW_MODE":"grid",
                 "HYPERCAST_SMOKE_LAYOUTS":"1","HYPERCAST_SMOKE_SECONDS":"16"})
        time.sleep(3)
        screenshot(gui.pid,ROOT/".tools/layout-grid.png")
        time.sleep(3)
        screenshot(gui.pid,ROOT/".tools/layout-spotlight.png")
        output,_ = gui.communicate(timeout=35)
        output=output.decode("utf-8",errors="replace")
        print(output.replace(token,"[token]"))
        assert gui.returncode == 0
        result=re.search(r"GUI smoke: (\d+) received, (\d+) GPU presentations; error: (.*)",output)
        assert result and int(result.group(1)) > 100 and int(result.group(2)) > 100
        assert not result.group(3).strip(),"GPU presentation error"
        for stage in ("spotlight hc-screen-2","paused","resumed grid"):
            assert "GUI layout: "+stage in output
        active=rt.participants(room,key,secret)
        assert all(f"hc-screen-{i}" in active for i in range(1,4)),"Publishers disconnected"
        print("PASS: three live publishers; GPU Grid, focus switch, pause and resume")
    finally:
        for process in [gui,*publishers]:
            if process and process.poll() is None:
                process.terminate(); process.wait(timeout=10)
        for log in logs:
            log.close()

if __name__ == "__main__":
    main()
