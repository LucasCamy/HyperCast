"""Generate deployment credentials locally, without printing secrets."""
import os
from pathlib import Path
import secrets

ROOT = Path(__file__).resolve().parent.parent

def main():
    path = ROOT / ".env"
    fd = os.open(path, os.O_WRONLY | os.O_CREAT | os.O_EXCL, 0o600)
    with os.fdopen(fd, "w", encoding="utf-8") as stream:
        stream.write(f"LIVEKIT_API_KEY=hc_{secrets.token_hex(12)}\n")
        stream.write(f"LIVEKIT_API_SECRET={secrets.token_hex(32)}\n")
        stream.write("LIVEKIT_URL=http://127.0.0.1:8100\nHYPERCAST_MAX_PUBLISHERS=3\n")
    print("Created .env; credentials were not printed. Existing files are never overwritten.")

if __name__ == "__main__":
    main()

