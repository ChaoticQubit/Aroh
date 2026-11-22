import base64
import hashlib
import hmac
import json
import secrets
import time


def b64url(raw):
    """URL-safe base64 without padding, as JWTs use it.
    Input: bytes. Output: str."""
    return base64.urlsafe_b64encode(raw).rstrip(b"=").decode()


def jwt(secret, claims):
    """Signs `claims` as an HS256 JWT with `secret`.
    Input: the shared secret and a claims dict. Output: the compact token."""
    head = b64url(json.dumps({"alg": "HS256", "typ": "JWT"}).encode())
    body = b64url(json.dumps(claims).encode())
    sig = hmac.new(secret.encode(), f"{head}.{body}".encode(), hashlib.sha256).digest()
    return f"{head}.{body}.{b64url(sig)}"


def main():
    """Prints fresh local-stack secrets as shell assignments: the JWT secret,
    the engine master key, and anon and service-role keys signed with it.
    Local test values only, regenerated per stack."""
    secret = secrets.token_hex(32)
    now = int(time.time())
    claims = {"iss": "supabase", "iat": now, "exp": now + 10 * 365 * 86400}
    values = {
        "AROH_E2E_JWT_SECRET": secret,
        "AROH_E2E_MASTER_KEY": base64.b64encode(secrets.token_bytes(32)).decode(),
        "AROH_E2E_ANON_KEY": jwt(secret, {**claims, "role": "anon"}),
        "AROH_E2E_SERVICE_KEY": jwt(secret, {**claims, "role": "service_role"}),
    }
    assert len(values["AROH_E2E_JWT_SECRET"]) == 64
    for k, v in values.items():
        print(f"{k}='{v}'")


if __name__ == "__main__":
    main()
