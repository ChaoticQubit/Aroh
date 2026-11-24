#!/bin/sh
set -eu
here="$(cd "$(dirname "$0")" && pwd)"
export HOME="${KIRI_HOME:-$HOME}"
project="aroh-server-test-$$"
cleanup() { kiri compose -p "$project" -f "$here/compose.yaml" down -v --rmi local >/dev/null 2>&1 || true; }
trap cleanup EXIT INT TERM
kiri compose -p "$project" -f "$here/compose.yaml" up --build --abort-on-container-exit --exit-code-from test
