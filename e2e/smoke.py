import json
import os
import sys
import time
import urllib.error
import urllib.request
import uuid

SUPABASE = "http://127.0.0.1:" + os.environ.get("AROH_E2E_SUPABASE_PORT", "54321")
ENGINE = "http://127.0.0.1:" + os.environ.get("AROH_E2E_ENGINE_PORT", "8080")


def call(method, url, token=None, body=None, anon=None, prefer=None):
    """One HTTP call returning (status, parsed JSON or None).
    Input: method, URL, optional bearer token, JSON body, apikey and Prefer.
    Output: the status code and the decoded body."""
    req = urllib.request.Request(url, method=method, data=None if body is None else json.dumps(body).encode())
    req.add_header("content-type", "application/json")
    if anon:
        req.add_header("apikey", anon)
    if token:
        req.add_header("authorization", f"Bearer {token}")
    if prefer:
        req.add_header("prefer", prefer)
    try:
        with urllib.request.urlopen(req, timeout=20) as res:
            text = res.read().decode()
            return res.status, json.loads(text) if text else None
    except urllib.error.HTTPError as e:
        text = e.read().decode()
        return e.code, json.loads(text) if text else None


def sign_up(anon):
    """Creates a fresh autoconfirmed user and returns (user id, access token).
    Input: the anon key. Output: the tuple."""
    email = f"smoke-{uuid.uuid4().hex[:10]}@aroh.test"
    status, body = call("POST", f"{SUPABASE}/auth/v1/signup", anon=anon, body={"email": email, "password": "smoke-password-1"})
    assert status == 200 and body.get("access_token"), (status, body)
    return body["user"]["id"], body["access_token"]


def main():
    """Smoke-checks the local stack: GoTrue sign-up, RLS isolation through
    PostgREST, engine health and an authenticated engine call.
    Exits non-zero on the first failure."""
    anon = os.environ["AROH_E2E_ANON_KEY"]
    a, ta = sign_up(anon)
    _, tb = sign_up(anon)
    stamp = f"{int(time.time() * 1000):013d}-0000-00000000000000a1"
    goal = {"id": str(uuid.uuid4()), "user_id": a, "title": "Smoke goal", "status": "draft", "autonomy": "balanced",
            "created_at": "2026-11-22T00:00:00Z", "updated_at": stamp, "deleted": False}
    status, body = call("POST", f"{SUPABASE}/rest/v1/goals", ta, [goal], anon, "return=minimal")
    assert status == 201, (status, body)
    _, mine = call("GET", f"{SUPABASE}/rest/v1/goals?select=id", ta, anon=anon)
    _, theirs = call("GET", f"{SUPABASE}/rest/v1/goals?select=id", tb, anon=anon)
    assert len(mine) == 1 and theirs == [], (mine, theirs)
    assert call("GET", f"{ENGINE}/healthz")[0] == 200
    status, keys = call("GET", f"{ENGINE}/v1/keys", ta)
    assert (status, keys) == (200, []), (status, keys)
    print("smoke ok: sign-up, RLS isolation, engine health and auth")


if __name__ == "__main__":
    sys.exit(main())
