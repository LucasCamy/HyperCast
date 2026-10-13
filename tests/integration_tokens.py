"""Runs against an actual local SFU; emits no tokens/secrets.
Run after docker compose up. Tests token signature with RoomService by using a
valid administrator token and a tampered signature, then tests admission.
"""
import json
from pathlib import Path
import tempfile
import time
import urllib.error
import urllib.request
from tools import room_token as rt

def main():
    key, secret = rt.load_env()
    room = "hc-integration-" + str(int(time.time()))
    assert rt.participants(room, key, secret) == set()
    with tempfile.TemporaryDirectory() as directory:
        db = Path(directory) / "admission.sqlite3"
        for _ in range(3): rt.issue(room, "test", "publisher", key, secret, db)
        try:
            rt.issue(room, "test", "publisher", key, secret, db)
        except RuntimeError:
            pass
        else:
            raise AssertionError("Fourth admission was not rejected")
        rt.issue(room, "test", "viewer", key, secret, db)
    now = int(time.time())
    good = rt.sign({"iss": key, "sub": "test-admin", "nbf": now - 5, "exp": now + 60,
                    "video": {"room": room, "roomAdmin": True}}, secret)
    parts = good.split(".")
    parts[2] = ("A" if parts[2][0] != "A" else "B") + parts[2][1:]
    url = rt.os.environ.get("LIVEKIT_URL", "http://127.0.0.1:8100")
    request = urllib.request.Request(url + "/twirp/livekit.RoomService/ListParticipants",
        data=json.dumps({"room": room}).encode(),
        headers={"Content-Type": "application/json", "Authorization": "Bearer " + ".".join(parts)})
    try:
        urllib.request.urlopen(request, timeout=5)
    except urllib.error.HTTPError as error:
        assert error.code == 401, error.code
    else:
        raise AssertionError("Invalid signature accepted")
    print("PASS: real SFU admin authentication, tampered JWT rejection, three reservations, fourth rejected, viewer permitted")

if __name__ == "__main__":
    main()
