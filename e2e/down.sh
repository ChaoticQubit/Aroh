#!/bin/sh
set -eu
here="$(cd "$(dirname "$0")" && pwd)"
state="$here/.local-state"
pod="${AROH_E2E_POD:-aroh-p11}"
supabase_port="${AROH_E2E_SUPABASE_PORT:-54321}"
engine_port="${AROH_E2E_ENGINE_PORT:-8080}"
export AROH_E2E_POD="$pod" AROH_E2E_SUPABASE_PORT="$supabase_port" AROH_E2E_ENGINE_PORT="$engine_port"
[ -f "$state/env" ] && . "$state/env"
export AROH_E2E_JWT_SECRET="${AROH_E2E_JWT_SECRET:-x}" AROH_E2E_MASTER_KEY="${AROH_E2E_MASTER_KEY:-x}"
kiri compose -p "$pod" -f "$here/compose.yaml" down -v --remove-orphans
if [ "${1:-}" = "--all" ]; then
  kiri volume rm "$pod-target" "$pod-cargo" >/dev/null 2>&1 || true
  rm -rf "$state"
fi
