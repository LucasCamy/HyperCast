"""Renew an existing HyperCast access key in the admission database."""

import argparse
from contextlib import closing
from datetime import datetime, timezone
import json
from pathlib import Path
import re
import sqlite3
import time


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("access_id", help="ID returned when the access key was issued")
    parser.add_argument("--days", type=int, default=30)
    parser.add_argument("--database", type=Path, default=Path("/data/admission.sqlite3"))
    args = parser.parse_args()
    if not re.fullmatch(r"[0-9a-f]{32}", args.access_id):
        parser.error("Access ID must have 32 lowercase hexadecimal characters")
    if not 1 <= args.days <= 90:
        parser.error("Renewal must be between 1 and 90 days")

    database = args.database.resolve().as_uri() + "?mode=rw"
    with closing(sqlite3.connect(database, uri=True, timeout=10)) as conn, conn:
        conn.execute("BEGIN IMMEDIATE")
        record = conn.execute(
            "SELECT expires, revoked FROM creators WHERE id=?", (args.access_id,)
        ).fetchone()
        if record is None:
            parser.exit(1, "Access ID was not found\n")
        previous, revoked = record
        if revoked:
            parser.exit(1, "Access key has been revoked\n")

        expires = max(previous, int(time.time()) + args.days * 86400)
        if expires > previous:
            conn.execute("UPDATE creators SET expires=? WHERE id=?", (expires, args.access_id))

    print(json.dumps({
        "expires_at": datetime.fromtimestamp(expires, timezone.utc).isoformat(timespec="seconds"),
        "changed": expires > previous,
    }))


if __name__ == "__main__":
    main()
