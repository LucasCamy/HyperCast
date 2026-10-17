"""Open the packaged PipeWire source through an FD, without desktop/audio capture.

Run on Linux: python3 -m tests.integration_pipewire_appimage --appimage /path/AppImage
Uses a private PipeWire daemon with no devices, sources or session manager.
"""
import argparse
import ctypes
import os
from pathlib import Path
import socket
import subprocess
import sys
import tempfile
import time


def probe(fd):
    import resource
    resource.setrlimit(resource.RLIMIT_CORE, (0, 0))
    gst = ctypes.CDLL("libgstreamer-1.0.so.0")
    gst.gst_init.argtypes = [ctypes.c_void_p, ctypes.c_void_p]
    gst.gst_init.restype = None
    gst.gst_parse_launch.argtypes = [ctypes.c_char_p, ctypes.POINTER(ctypes.c_void_p)]
    gst.gst_parse_launch.restype = ctypes.c_void_p
    gst.gst_element_set_state.argtypes = [ctypes.c_void_p, ctypes.c_int]
    gst.gst_element_set_state.restype = ctypes.c_int
    gst.gst_element_get_state.argtypes = [ctypes.c_void_p, ctypes.POINTER(ctypes.c_int), ctypes.POINTER(ctypes.c_int), ctypes.c_uint64]
    gst.gst_element_get_state.restype = ctypes.c_int
    gst.gst_object_unref.argtypes = [ctypes.c_void_p]
    gst.gst_object_unref.restype = None
    gst.gst_init(None, None)
    error = ctypes.c_void_p()
    pipeline = gst.gst_parse_launch(f"pipewiresrc fd={fd} ! fakesink".encode(), ctypes.byref(error))
    if not pipeline or error.value:
        raise RuntimeError("Cannot construct packaged PipeWire pipeline")
    try:
        # NULL -> READY creates the PipeWire context and stream. No source
        # is connected and no frames are captured. This crashed in 0.8.9.
        assert gst.gst_element_set_state(pipeline, 2) != 0, "PipeWire READY failed"
        state, pending = ctypes.c_int(), ctypes.c_int()
        assert gst.gst_element_get_state(pipeline, ctypes.byref(state), ctypes.byref(pending), 2_000_000_000) != 0
        assert state.value == 2, "PipeWire did not reach READY"
    finally:
        gst.gst_element_set_state(pipeline, 1)
        gst.gst_object_unref(pipeline)


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--appimage", type=Path)
    parser.add_argument("--probe-fd", type=int)
    args = parser.parse_args()
    if args.probe_fd is not None:
        return probe(args.probe_fd)
    if not args.appimage:
        parser.error("--appimage is required")
    image = args.appimage.resolve()
    with tempfile.TemporaryDirectory(prefix="hypercast-pipewire-") as directory:
        work = Path(directory)
        subprocess.run([str(image), "--appimage-extract"], cwd=work, check=True, stdout=subprocess.DEVNULL, timeout=60)
        app = work / "squashfs-root"
        lib = app / "usr/lib"
        runtime = work / "runtime"
        runtime.mkdir(mode=0o700)
        config = work / "isolated-server.conf"
        config.write_text('''context.properties = { core.daemon = true core.name = pipewire-0 }
context.spa-libs = { support.* = support/libspa-support }
context.modules = [
    { name = libpipewire-module-protocol-native }
    { name = libpipewire-module-client-node }
    { name = libpipewire-module-access }
]
''')
        env = {k: v for k, v in os.environ.items() if not k.startswith(("PIPEWIRE_", "SPA_", "GST_")) and k != "LD_LIBRARY_PATH"}
        env["XDG_RUNTIME_DIR"] = str(runtime)
        with (work / "daemon.log").open("w") as daemon_log:
            server = subprocess.Popen(["pipewire", "-c", str(config)], env=env, stdout=daemon_log, stderr=subprocess.STDOUT)
            try:
                for _ in range(100):
                    if (runtime / "pipewire-0").exists():
                        break
                    if server.poll() is not None:
                        raise RuntimeError("Isolated PipeWire daemon failed: " + (work / "daemon.log").read_text())
                    time.sleep(0.05)
                child_env = {**env, "LD_LIBRARY_PATH": str(lib), "GST_PLUGIN_SYSTEM_PATH_1_0": "", "GST_PLUGIN_PATH_1_0": str(lib / "gstreamer-1.0"), "GST_PLUGIN_SCANNER": str(lib / "gst-plugin-scanner"), "GST_REGISTRY": str(work / "registry.bin"), "SPA_PLUGIN_DIR": str(lib / "spa-0.2"), "PIPEWIRE_MODULE_DIR": str(lib / "pipewire-0.3"), "PIPEWIRE_CONFIG_DIR": str(app / "usr/share/pipewire")}
                for _ in range(3):
                    with socket.socket(socket.AF_UNIX) as remote:
                        remote.connect(str(runtime / "pipewire-0"))
                        result = subprocess.run([sys.executable, str(Path(__file__).resolve()), "--probe-fd", str(remote.fileno())], env=child_env, pass_fds=(remote.fileno(),), timeout=20)
                        assert result.returncode == 0, f"Packaged PipeWire probe failed: exit {result.returncode}"
                print("PASS: three packaged PipeWire FD open/close cycles; no capture")
            finally:
                server.terminate()
                try:
                    server.wait(timeout=5)
                except subprocess.TimeoutExpired:
                    server.kill()
                    server.wait()


if __name__ == "__main__":
    main()
