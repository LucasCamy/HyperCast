import unittest
from pathlib import Path

from tools.deploy_allow_pause_on_node import pause_config

ROOT = Path(__file__).resolve().parent.parent
LIVE = """rtc:
  congestion_control:
    enabled: true
    allow_pause: true
room:
  auto_create: true
"""


class PauseConfigTests(unittest.TestCase):
    def test_disables_only_allow_pause(self):
        self.assertEqual(pause_config(LIVE), LIVE.replace('allow_pause: true', 'allow_pause: false'))

    def test_is_idempotent(self):
        once = pause_config(LIVE)
        self.assertEqual(pause_config(once), once)

    def test_rejects_unexpected_layout(self):
        with self.assertRaises(RuntimeError):
            pause_config('rtc:\n  port: 1\n')

    def test_repository_configs_never_pause(self):
        for name in ('livekit.yaml', 'livekit.dev.yaml'):
            text = (ROOT / name).read_text(encoding='utf-8')
            self.assertIn('    allow_pause: false\n', text, name)
            self.assertEqual(pause_config(text), text, name)


if __name__ == '__main__':
    unittest.main()
