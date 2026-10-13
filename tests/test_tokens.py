import base64
from contextlib import closing
from concurrent.futures import ThreadPoolExecutor
import hashlib
import hmac
import json
from pathlib import Path
import sqlite3
import tempfile
import unittest
from unittest.mock import patch

from tools import room_token as rt

SECRET = "test-only-" + "s" * 40

def decode(token):
    header, body, signature = token.split(".")
    expected = hmac.new(SECRET.encode(), (header + "." + body).encode(), hashlib.sha256).digest()
    assert hmac.compare_digest(expected, base64.urlsafe_b64decode(signature + "=" * (-len(signature) % 4)))
    return json.loads(base64.urlsafe_b64decode(body + "=" * (-len(body) % 4)))

class AdmissionTests(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory()
        self.addCleanup(self.temp.cleanup)
        self.path = Path(self.temp.name) / "admission.sqlite3"

    def issue(self, role="publisher", cap=3):
        return rt.issue("friends", "Alice", role, "test", SECRET, self.path, cap)

    @patch.object(rt, "participants", return_value=set())
    def test_three_publishers_fourth_rejected_and_viewer_allowed(self, _):
        claims = [decode(self.issue()) for _ in range(3)]
        self.assertEqual({c["sub"] for c in claims}, {"hc-screen-1", "hc-screen-2", "hc-screen-3"})
        self.assertEqual(claims[0]["video"]["canPublishSources"], ["screen_share"])
        with self.assertRaises(RuntimeError): self.issue()
        viewer = decode(self.issue("viewer"))
        self.assertFalse(viewer["video"]["canPublish"])
        self.assertFalse(viewer["video"]["canPublishData"])

    @patch.object(rt, "participants", return_value=set())
    def test_atomic_concurrent_admission(self, _):
        def attempt(_index):
            try: return decode(self.issue())["sub"]
            except RuntimeError: return None
        with ThreadPoolExecutor(max_workers=8) as pool:
            outcomes = list(pool.map(attempt, range(12)))
        successful = [x for x in outcomes if x]
        self.assertEqual(len(successful), 3)
        self.assertEqual(len(set(successful)), 3)

    def test_connected_sessions_still_occupy_slots_after_token_expiry(self):
        with patch.object(rt, "participants", return_value={"hc-screen-1", "hc-screen-2", "hc-screen-3"}):
            with self.assertRaises(RuntimeError): self.issue()

    @patch.object(rt, "participants", side_effect=OSError("SFU unavailable"))
    def test_sfu_failure_does_not_admit(self, _):
        with self.assertRaises(OSError): self.issue()
        self.assertFalse(self.path.exists())

    @patch.object(rt, "participants", return_value=set())
    def test_reducing_cap_requires_draining_leases(self, _):
        for _ in range(3): self.issue()
        with self.assertRaises(RuntimeError): self.issue(cap=2)

    def test_expired_reservation_can_be_reused_but_active_identity_cannot(self):
        with closing(sqlite3.connect(self.path, isolation_level=None)) as db:
            self.assertEqual(rt.reserve(db, "friends", 3, set(), 100, 200), "hc-screen-1")
            self.assertEqual(rt.reserve(db, "friends", 3, {"hc-screen-1"}, 201, 300), "hc-screen-2")

    def test_validation_and_viewer_need_no_admin_request(self):
        with self.assertRaises(ValueError): rt.issue("../room", "Alice", "viewer", "test", SECRET, self.path)
        with patch.object(rt, "participants", side_effect=AssertionError("Unexpected request")):
            self.assertTrue(decode(self.issue("viewer"))["video"]["canSubscribe"])

if __name__ == "__main__":
    unittest.main()

