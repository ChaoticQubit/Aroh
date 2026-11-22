#!/bin/sh
set -eu
here="$(cd "$(dirname "$0")" && pwd)"
state="$here/.local-state"
[ -f "$state/env" ] && . "$state/env"
export AROH_E2E_JWT_SECRET="${AROH_E2E_JWT_SECRET:-x}" AROH_E2E_MASTER_KEY="${AROH_E2E_MASTER_KEY:-x}"
kiri compose -p aroh-p11 -f "$here/compose.yaml" down -v --remove-orphans
if [ "${1:-}" = "--all" ]; then
  kiri volume rm aroh-p11-target aroh-p11-cargo >/dev/null 2>&1 || true
  rm -rf "$state"
fi
