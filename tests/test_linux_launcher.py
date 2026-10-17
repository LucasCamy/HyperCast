"""Exercise AppRun's actual process/log behavior without loading GPU drivers."""
import os
from pathlib import Path
import shutil
import subprocess
import tempfile
import unittest


@unittest.skipUnless(os.name == "posix", "AppRun requires a POSIX shell")
class LauncherTest(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory(prefix="hypercast launcher ")
        self.addCleanup(self.temp.cleanup)
        self.root = Path(self.temp.name)
        self.app = self.root / "App Dir"
        binary = self.app / "usr/bin/hypercast"
        binary.parent.mkdir(parents=True)
        binary.write_text('#!/bin/sh\necho child-start\ncase "${TEST_EXIT:-0}" in\n signal) kill -TERM $$;;\n *) exit "${TEST_EXIT:-0}";;\nesac\n')
        binary.chmod(0o755)
        shutil.copyfile(Path(__file__).resolve().parents[1] / "tools/linux/AppRun", self.app / "AppRun")
        self.env = {**os.environ, "APPDIR": str(self.app), "HOME": str(self.root), "XDG_CACHE_HOME": str(self.root / "custom cache")}
        self.log = self.root / "custom cache/hypercast/hypercast.log"

    def launch(self, *args, **env):
        return subprocess.run(["sh", str(self.app / "AppRun"), *args], env={**self.env, **env}, capture_output=True, text=True, timeout=10)

    def test_both_gui_entry_points_log_and_preserve_failure(self):
        for args in [(), ("--gui",)]:
            with self.subTest(args=args):
                self.log.unlink(missing_ok=True)
                result = self.launch(*args, TEST_EXIT="7")
                self.assertEqual(result.returncode, 7)
                self.assertIn("child-start", self.log.read_text())
                self.assertIn("exit_code=7", self.log.read_text())
                self.assertEqual(self.log.stat().st_mode & 0o777, 0o600)

    def test_native_signal_leaves_exit_status(self):
        result = self.launch("--gui", TEST_EXIT="signal")
        self.assertEqual(result.returncode, 143)
        self.assertIn("exit_code=143", self.log.read_text())

    def test_cli_remains_visible_and_arguments_are_not_logged(self):
        result = self.launch("--linux-media-test", TEST_EXIT="3")
        self.assertEqual(result.returncode, 3)
        self.assertIn("child-start", result.stdout)
        self.assertFalse(self.log.exists())
        self.launch("--gui", "--private-value", "must-not-be-logged")
        self.assertNotIn("must-not-be-logged", self.log.read_text())


if __name__ == "__main__":
    unittest.main()
