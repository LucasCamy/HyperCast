import os
from pathlib import Path
import tempfile
import unittest
from unittest.mock import patch
from tools.room_token import load_env


class OperatorEnvironmentTests(unittest.TestCase):
    def test_explicit_target_overrides_development_credentials(self):
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory) / "operator.env"
            path.write_text("LIVEKIT_API_KEY=test-production\nLIVEKIT_API_SECRET=" + "p" * 48 + "\n", encoding="utf-8")
            with patch.dict(os.environ, {"LIVEKIT_API_KEY": "test-development", "LIVEKIT_API_SECRET": "d" * 48}):
                key, secret = load_env(path)
                self.assertEqual(key, "test-production")
                self.assertEqual(secret, "p" * 48)

    def test_missing_explicit_file_does_not_fall_back(self):
        with tempfile.TemporaryDirectory() as directory:
            with self.assertRaises(FileNotFoundError):
                load_env(Path(directory) / "missing.env")
