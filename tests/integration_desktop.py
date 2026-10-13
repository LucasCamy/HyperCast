"""GPU GUI smoke test. Captures only HyperCast's own window into ignored .tools.

Requires Pillow for the optional layout screenshot (pip --target .tools/python-libs).
The publisher captures the selected desktop; no encoded/raw screen recording is saved.
"""
import ctypes
from ctypes import wintypes
import os
from pathlib import Path
import re
import subprocess
import sys
import time
import uuid
from tools import room_token as rt

ROOT = Path(__file__).resolve().parent.parent

def owned_window(pid):
    user = ctypes.WinDLL("user32", use_last_error=True)
    callback_type = ctypes.WINFUNCTYPE(wintypes.BOOL, wintypes.HWND, wintypes.LPARAM)
    windows = []
    def visit(hwnd, _):
        owner = wintypes.DWORD()
        user.GetWindowThreadProcessId(hwnd, ctypes.byref(owner))
        if owner.value == pid and user.IsWindowVisible(hwnd):
            windows.append(hwnd)
        return True
    user.EnumWindows(callback_type(visit), 0)
    assert windows, "GUI has no visible window"
    return user, windows[0]

def screenshot(pid, destination):
    sys.path.insert(0, str(ROOT / ".tools/python-libs"))
    from PIL import Image
    user, hwnd = owned_window(pid)
    rect = wintypes.RECT()
    assert user.GetClientRect(hwnd, ctypes.byref(rect))
    width,height=rect.right,rect.bottom
    assert 100 <= width <= 8192 and 100 <= height <= 8192
    # Print only the process-owned HWND, even when another app is foreground.
    # This avoids accidentally screenshotting an overlapping unrelated window.
    gdi=ctypes.WinDLL("gdi32",use_last_error=True)
    user.GetDC.argtypes=[wintypes.HWND]; user.GetDC.restype=wintypes.HDC
    user.ReleaseDC.argtypes=[wintypes.HWND,wintypes.HDC]
    user.PrintWindow.argtypes=[wintypes.HWND,wintypes.HDC,wintypes.UINT]
    gdi.CreateCompatibleDC.argtypes=[wintypes.HDC]; gdi.CreateCompatibleDC.restype=wintypes.HDC
    gdi.CreateCompatibleBitmap.argtypes=[wintypes.HDC,ctypes.c_int,ctypes.c_int]; gdi.CreateCompatibleBitmap.restype=wintypes.HBITMAP
    gdi.SelectObject.argtypes=[wintypes.HDC,wintypes.HGDIOBJ]; gdi.SelectObject.restype=wintypes.HGDIOBJ
    gdi.DeleteObject.argtypes=[wintypes.HGDIOBJ]; gdi.DeleteDC.argtypes=[wintypes.HDC]
    class Header(ctypes.Structure):
        _fields_=[("size",wintypes.DWORD),("width",wintypes.LONG),("height",wintypes.LONG),
            ("planes",wintypes.WORD),("bits",wintypes.WORD),("compression",wintypes.DWORD),
            ("image_size",wintypes.DWORD),("xppm",wintypes.LONG),("yppm",wintypes.LONG),
            ("used",wintypes.DWORD),("important",wintypes.DWORD)]
    class Info(ctypes.Structure):
        _fields_=[("header",Header),("colors",wintypes.DWORD*3)]
    gdi.GetDIBits.argtypes=[wintypes.HDC,wintypes.HBITMAP,wintypes.UINT,wintypes.UINT,ctypes.c_void_p,ctypes.POINTER(Info),wintypes.UINT]
    screen=user.GetDC(hwnd); memory=None; bitmap=None; previous=None
    try:
        assert screen
        memory=gdi.CreateCompatibleDC(screen); bitmap=gdi.CreateCompatibleBitmap(screen,width,height)
        assert memory and bitmap
        previous=gdi.SelectObject(memory,bitmap)
        assert user.PrintWindow(hwnd,memory,3), "PrintWindow failed"
        gdi.SelectObject(memory,previous); previous=None
        info=Info(Header(40,width,-height,1,32,0,width*height*4,0,0,0,0))
        pixels=ctypes.create_string_buffer(width*height*4)
        assert gdi.GetDIBits(memory,bitmap,0,height,pixels,ctypes.byref(info),0)==height
        Image.frombytes("RGB",(width,height),pixels.raw,"raw","BGRX").save(destination)
    finally:
        if previous: gdi.SelectObject(memory,previous)
        if bitmap: gdi.DeleteObject(bitmap)
        if memory: gdi.DeleteDC(memory)
        if screen: user.ReleaseDC(hwnd,screen)

def main():
    key, secret = rt.load_env()
    room = "gui-test-" + uuid.uuid4().hex[:12]
    exe = ROOT / "target/x86_64-pc-windows-msvc/debug/hypercast.exe"
    publisher_token = rt.issue(room, "Lucas", "publisher", key, secret, ROOT/"infra/admission.sqlite3")
    viewer_token = rt.issue(room, "Viewer", "viewer", key, secret, ROOT/"infra/admission.sqlite3")
    pub = subprocess.Popen([str(exe),"--encode","--codec","av1","--publish-url","ws://127.0.0.1:8100","--seconds","24"],
        cwd=ROOT, env={**os.environ,"HYPERCAST_ROOM_TOKEN":publisher_token}, stdout=subprocess.PIPE, stderr=subprocess.STDOUT)
    gui = None
    try:
        gui = subprocess.Popen([str(exe),"--gui"], cwd=ROOT, stdout=subprocess.PIPE,stderr=subprocess.STDOUT,
            env={**os.environ,"HYPERCAST_ROOM_TOKEN":viewer_token,"HYPERCAST_AUTO_CONNECT":"1","HYPERCAST_SMOKE_SECONDS":"14","HYPERCAST_SMOKE_RECONNECT":"1"})
        time.sleep(3)
        user, hwnd = owned_window(gui.pid)
        user.ShowWindow(hwnd, 6)  # Minimize only the child process owned by this test.
        time.sleep(1.5)
        user.ShowWindow(hwnd, 9)
        time.sleep(5.5)
        screenshot(gui.pid, ROOT/".tools/layout-connected.png")
        output,_ = gui.communicate(timeout=35)
        output = output.decode("utf-8",errors="replace")
        print(output.replace(viewer_token,"[token]"))
        assert gui.returncode == 0, "GUI failed"
        result = re.search(r"GUI smoke: (\d+) received, (\d+) GPU presentations; error: (.*)",output)
        assert result and int(result.group(1)) > 10 and int(result.group(2)) > 10
        assert not result.group(3).strip(), "D3D11 presentation failed"
        assert "GUI lifecycle: disconnected and restarted media engine" in output
        assert "GUI lifecycle: 0 retained GPU decoder surfaces after disconnect" in output
        hidden=re.search(r"GUI viewport: minimized at (\d+) frames",output)
        restored=re.search(r"GUI viewport: restored at (\d+) frames",output)
        assert hidden and restored, "OS minimization was not detected by the media worker"
        assert int(restored.group(1))-int(hidden.group(1)) <= 30, "Decode continued while the viewport was minimized"
        print("PASS: native desktop receives and presents GPU frames; layout screenshot .tools/layout-connected.png")
    finally:
        for process in (gui,pub):
            if process and process.poll() is None:
                process.terminate()
                process.wait(timeout=10)

if __name__ == "__main__":
    main()
