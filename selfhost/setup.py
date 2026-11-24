"""Writes selfhost/.env once with fresh secrets for the self-host stack.

Usage: `python3 selfhost/setup.py https://aroh.example.com`. The argument is the public https
address the gateway is reached at (or http://localhost:8000 for a local trial). Refuses to
overwrite an existing .env, so secrets never change under a running database.
"""

import base64
import hashlib
import hmac
import json
import pathlib
import secrets
import sys
import time

ENV = pathlib.Path(__file__).with_name(".env")
TEN_YEARS = 10 * 365 * 86400


def b64url(raw):
    """Returns `raw` bytes as URL-safe base64 without padding, as JWTs use it."""
    return base64.urlsafe_b64encode(raw).rstrip(b"=").decode()


def jwt(secret, claims):
    """Signs the `claims` dict as an HS256 JWT with `secret` and returns the compact token."""
    head = b64url(json.dumps({"alg": "HS256", "typ": "JWT"}).encode())
    body = b64url(json.dumps(claims).encode())
    sig = hmac.new(secret.encode(), f"{head}.{body}".encode(), hashlib.sha256).digest()
    return f"{head}.{body}.{b64url(sig)}"


def values(public_url):
    """Returns the .env entries for `public_url`: database password, JWT secret, the anon and
    service role keys signed with it, the engine master key and the Realtime keys."""
    secret = secrets.token_hex(32)
    now = int(time.time())
    claims = {"iss": "supabase", "iat": now, "exp": now + TEN_YEARS}
    out = {
        "PUBLIC_URL": public_url,
        "POSTGRES_PASSWORD": secrets.token_hex(24),
        "JWT_SECRET": secret,
        "ANON_KEY": jwt(secret, {**claims, "role": "anon"}),
        "SERVICE_ROLE_KEY": jwt(secret, {**claims, "role": "service_role"}),
        "AROH_MASTER_KEY": base64.b64encode(secrets.token_bytes(32)).decode(),
        "REALTIME_DB_ENC_KEY": secrets.token_hex(8),
        "REALTIME_SECRET_KEY_BASE": secrets.token_hex(64),
    }
    assert len(out["JWT_SECRET"]) == 64 and len(out["REALTIME_DB_ENC_KEY"]) == 16
    assert len(base64.b64decode(out["AROH_MASTER_KEY"])) == 32
    return out


def main(argv):
    """Checks the public URL argument, then writes .env with mode 0600 unless one exists."""
    if len(argv) != 2 or not argv[1].startswith(("https://", "http://localhost", "http://127.0.0.1")):
        sys.exit("usage: python3 selfhost/setup.py https://<your host>")
    if ENV.exists():
        sys.exit(f"{ENV} exists; delete it only if the database is empty")
    lines = [f"{k}={v}" for k, v in values(argv[1].rstrip("/")).items()]
    ENV.touch(mode=0o600)
    ENV.write_text("\n".join(lines) + "\n")
    print(f"wrote {ENV}; secrets are not printed")


if __name__ == "__main__":
    main(sys.argv)
