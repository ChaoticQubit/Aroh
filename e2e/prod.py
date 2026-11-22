import json
import os
import sys
import time
import urllib.error
import urllib.request

from keys import jwt

USAGE = "usage: python3 e2e/prod.py mint | smoke"
SERVICE_DAYS = 365


def need(name):
    """Returns the environment variable `name`, stopping with a message
    when it is unset or blank. Input: the name. Output: the value.
    Secrets come only from the environment, never from this file."""
    value = os.environ.get(name, "").strip()
    if not value:
        sys.exit(f"{name} is not set")
    return value


def call(method, url, token=None, body=None, apikey=None):
    """One HTTP call returning (status, parsed JSON or None).
    Input: method, URL, optional bearer token, JSON body and apikey.
    Output: the status code and the decoded body."""
    data = None if body is None else json.dumps(body).encode()
    req = urllib.request.Request(url, method=method, data=data)
    req.add_header("content-type", "application/json")
    if apikey:
        req.add_header("apikey", apikey)
    if token:
        req.add_header("authorization", f"Bearer {token}")
    try:
        with urllib.request.urlopen(req, timeout=300) as res:
            text = res.read().decode()
            return res.status, json.loads(text) if text else None
    except urllib.error.HTTPError as e:
        text = e.read().decode()
        try:
            return e.code, json.loads(text) if text else None
        except ValueError:
            return e.code, text


def mint():
    """Prints a service-role JWT for pg_cron, signed HS256 with
    AROH_JWT_SECRET and valid for SERVICE_DAYS days.
    Input: the env var. Output: the token on stdout, nothing else."""
    now = int(time.time())
    claims = {"role": "service_role", "iss": "aroh", "iat": now, "exp": now + SERVICE_DAYS * 86400}
    print(jwt(need("AROH_JWT_SECRET"), claims))


def check(label, got, want):
    """Prints one smoke line and records a failure.
    Input: the label, the status seen and the status wanted.
    Output: True when they match."""
    ok = got == want
    print(f"{'ok  ' if ok else 'FAIL'} {label}: {got} (want {want})")
    return ok


def smoke():
    """Checks a deployed engine: health, auth refusal, an authenticated
    read as the owner, the service-only guard, and with AROH_SERVICE_JWT
    the step-mode ticks. Input: env vars. Output: exit 0 when all pass."""
    engine = need("AROH_SERVER_URL").rstrip("/")
    supabase = need("SUPABASE_URL").rstrip("/")
    apikey = need("SUPABASE_ANON_KEY")
    login = {"email": need("AROH_SMOKE_EMAIL"), "password": need("AROH_SMOKE_PASSWORD")}
    status, body = call("POST", f"{supabase}/auth/v1/token?grant_type=password", body=login, apikey=apikey)
    if status != 200 or not isinstance(body, dict) or not body.get("access_token"):
        sys.exit(f"FAIL sign-in at {supabase}: {status}")
    user = body["access_token"]
    results = [
        check("GET /healthz", call("GET", f"{engine}/healthz")[0], 200),
        check("GET /v1/keys without a token", call("GET", f"{engine}/v1/keys")[0], 401),
        check("GET /v1/keys as the owner", call("GET", f"{engine}/v1/keys", token=user)[0], 200),
        check("POST /v1/tick as the owner", call("POST", f"{engine}/v1/tick", token=user, body={})[0], 403),
    ]
    service = os.environ.get("AROH_SERVICE_JWT", "").strip()
    if service:
        results.append(check("POST /v1/tick as the service", call("POST", f"{engine}/v1/tick", token=service, body={})[0], 200))
        results.append(check("POST /v1/push/tick as the service", call("POST", f"{engine}/v1/push/tick", token=service, body={})[0], 200))
    else:
        print("skip service ticks: AROH_SERVICE_JWT is not set")
    assert len(results) >= 4
    sys.exit(0 if all(results) else 1)


if __name__ == "__main__":
    command = sys.argv[1] if len(sys.argv) > 1 else ""
    if command == "mint":
        mint()
    elif command == "smoke":
        smoke()
    else:
        sys.exit(USAGE)
