#!/bin/sh
set -eu
here="$(cd "$(dirname "$0")" && pwd)"
state="$here/.local-state"
pod="${AROH_E2E_POD:-aroh-p11}"
supabase_port="${AROH_E2E_SUPABASE_PORT:-54321}"
engine_port="${AROH_E2E_ENGINE_PORT:-8080}"
export AROH_E2E_POD="$pod" AROH_E2E_SUPABASE_PORT="$supabase_port" AROH_E2E_ENGINE_PORT="$engine_port"
mkdir -p "$state"
[ -f "$state/env" ] || python3 "$here/keys.py" > "$state/env"
. "$state/env"
export AROH_E2E_JWT_SECRET AROH_E2E_MASTER_KEY
zone="${AROH_E2E_ZONE:-$(readlink /etc/localtime | sed 's|.*/zoneinfo/||')}"
python3 "$here/script.py" "$state/script.json" "$zone" "${AROH_E2E_REMINDER_MINUTES:-30}" "${AROH_E2E_FLOW:-drive.js}"
kiri volume create "$pod-target" >/dev/null
kiri volume create "$pod-cargo" >/dev/null
kiri compose -p "$pod" -f "$here/compose.yaml" up -d
tries=0
until curl -sf "http://127.0.0.1:$engine_port/healthz" >/dev/null; do
  tries=$((tries + 1))
  if [ "$tries" -gt 600 ]; then
    kiri compose -p "$pod" -f "$here/compose.yaml" logs --tail 40 engine migrate auth
    exit 1
  fi
  sleep 2
done
until curl -sf "http://127.0.0.1:$supabase_port/auth/v1/health" -H "apikey: $AROH_E2E_ANON_KEY" >/dev/null; do sleep 1; done
echo "stack up: supabase http://127.0.0.1:$supabase_port engine http://127.0.0.1:$engine_port"
