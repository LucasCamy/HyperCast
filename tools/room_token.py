"""Operator-only token CLI, stdlib only. Never distribute API secrets to clients.

Publishers occupy a fixed identity pool per room. LiveKit's single-identity rule
bounds publisher participants. The patched SFU separately rejects duplicate
and out-of-pool screen tracks synchronously, including pending publications.
SQLite rejects concurrent issuance above the cap, including not-yet-connected
token reservations. Use the custom SFU image, not an unmodified LiveKit image.
"""
import argparse
import base64
from contextlib import closing
import hashlib
import hmac
import json
import os
from pathlib import Path
import re
import sqlite3
import time
import urllib.parse
import urllib.request
import uuid

ROOT = Path(__file__).resolve().parent.parent
NAME = re.compile(r"[A-Za-z0-9_-]{1,64}\Z")

def load_env():
    path = ROOT / ".env"
    if path.exists():
        for line in path.read_text(encoding="utf-8").splitlines():
            if line and not line.startswith("#"):
                key, value = line.split("=", 1)
                os.environ.setdefault(key, value)
    key, secret = os.environ["LIVEKIT_API_KEY"], os.environ["LIVEKIT_API_SECRET"]
    if len(secret) < 32 or secret == "generated-by-tools-init_env":
        raise ValueError("Generate a strong API secret using tools/init_env.py")
    return key, secret

def sign(claims, secret):
    def b64(data):
        return base64.urlsafe_b64encode(data).rstrip(b"=")
    header = b64(b'{"alg":"HS256","typ":"JWT"}')
    body = b64(json.dumps(claims, separators=(",", ":")).encode())
    message = header + b"." + body
    return (message + b"." + b64(hmac.new(secret.encode(), message, hashlib.sha256).digest())).decode()

def participants(room, key, secret):
    url = os.environ.get("LIVEKIT_URL", "http://127.0.0.1:8100").rstrip("/")
    parsed = urllib.parse.urlparse(url)
    if parsed.scheme != "https" and not (parsed.scheme == "http" and parsed.hostname in {"127.0.0.1", "localhost", "::1"}):
        raise ValueError("RoomService must use HTTPS, or loopback HTTP")
    now = int(time.time())
    token = sign({"iss": key, "sub": "hypercast-operator", "nbf": now - 5,
                  "exp": now + 60, "video": {"room": room, "roomAdmin": True}}, secret)
    request = urllib.request.Request(url + "/twirp/livekit.RoomService/ListParticipants",
        data=json.dumps({"room": room}).encode(),
        headers={"Authorization": "Bearer " + token, "Content-Type": "application/json"})
    # No redirect: never forward an administrator bearer token to another origin.
    class NoRedirect(urllib.request.HTTPRedirectHandler):
        def redirect_request(self, *args, **kwargs):
            return None
    with urllib.request.build_opener(NoRedirect).open(request, timeout=5) as response:
        data = json.loads(response.read(1024 * 1024))
    return {p["identity"] for p in data.get("participants", [])}

def reserve(db, room, cap, active, now, expires):
    db.execute("CREATE TABLE IF NOT EXISTS leases (room TEXT, slot INTEGER, expires INTEGER, PRIMARY KEY(room, slot))")
    db.execute("BEGIN IMMEDIATE")
    try:
        db.execute("DELETE FROM leases WHERE expires <= ?", (now,))
        reserved = {row[0] for row in db.execute("SELECT slot FROM leases WHERE room = ?", (room,))}
        # Lowering the cap while sessions/leases exist could violate the new limit.
        if any(slot > cap for slot in reserved) or any(
            name.startswith("hc-screen-") and name not in {f"hc-screen-{i}" for i in range(1, cap + 1)}
            for name in active
        ):
            raise RuntimeError("Drain the room before reducing the publisher limit")
        for slot in range(1, cap + 1):
            identity = f"hc-screen-{slot}"
            if slot not in reserved and identity not in active:
                db.execute("INSERT INTO leases VALUES (?, ?, ?)", (room, slot, expires))
                db.commit()
                return identity
        raise RuntimeError("Room publisher limit reached (active sessions or unexpired reservations)")
    except Exception:
        db.rollback()
        raise

def issue(room, name, role, key, secret, db_path, cap=3, ttl=300):
    if not NAME.fullmatch(room) or not NAME.fullmatch(name):
        raise ValueError("Room and name must be 1-64 ASCII letters, digits, '_' or '-'")
    if not 1 <= cap <= 3 or not 30 <= ttl <= 300 or role not in {"viewer", "publisher"}:
        raise ValueError("Invalid cap, TTL or role")
    now = int(time.time())
    identity = "hc-viewer-" + uuid.uuid4().hex
    if role == "publisher":
        active = participants(room, key, secret)  # fail closed if SFU cannot be queried
        with closing(sqlite3.connect(db_path, timeout=10, isolation_level=None)) as db:
            identity = reserve(db, room, cap, active, now, now + ttl)
    claims = {"iss": key, "sub": identity, "name": name, "nbf": now - 5, "exp": now + ttl,
              "video": {"room": room, "roomJoin": True, "canSubscribe": True,
                        "canPublish": role == "publisher", "canPublishData": False,
                        "canUpdateOwnMetadata": False}}
    if role == "publisher":
        claims["video"]["canPublishSources"] = ["screen_share"]
    return sign(claims, secret)

def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--room", required=True)
    parser.add_argument("--name", required=True)
    parser.add_argument("--role", choices=["viewer", "publisher"], default="viewer")
    args = parser.parse_args()
    key, secret = load_env()
    print(issue(args.room, args.name, args.role, key, secret, ROOT / "infra/admission.sqlite3",
                cap=int(os.environ.get("HYPERCAST_MAX_PUBLISHERS", "3"))))

if __name__ == "__main__":
    main()
