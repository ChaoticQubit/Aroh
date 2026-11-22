#!/bin/sh
set -eu
here="$(cd "$(dirname "$0")" && pwd)"
state="$here/.local-state"
mkdir -p "$state"
[ -f "$state/env" ] || python3 "$here/keys.py" > "$state/env"
. "$state/env"
export AROH_E2E_JWT_SECRET AROH_E2E_MASTER_KEY
zone="${AROH_E2E_ZONE:-$(readlink /etc/localtime | sed 's|.*/zoneinfo/||')}"
python3 "$here/script.py" "$state/script.json" "$zone" "${AROH_E2E_REMINDER_MINUTES:-30}"
kiri volume create aroh-p11-target >/dev/null
kiri volume create aroh-p11-cargo >/dev/null
kiri compose -p aroh-p11 -f "$here/compose.yaml" up -d
tries=0
until curl -sf http://127.0.0.1:8080/healthz >/dev/null; do
  tries=$((tries + 1))
  if [ "$tries" -gt 600 ]; then
    kiri compose -p aroh-p11 -f "$here/compose.yaml" logs --tail 40 engine migrate auth
    exit 1
  fi
  sleep 2
done
until curl -sf http://127.0.0.1:54321/auth/v1/health -H "apikey: $AROH_E2E_ANON_KEY" >/dev/null; do sleep 1; done
echo "stack up: supabase http://127.0.0.1:54321 engine http://127.0.0.1:8080"
