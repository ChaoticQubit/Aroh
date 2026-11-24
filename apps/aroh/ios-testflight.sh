#!/bin/sh
set -eu
here="$(cd "$(dirname "$0")" && pwd)"
env_file="${AROH_RELEASE_ENV:-$here/.env.release}"
[ -f "$env_file" ] || { echo "missing $env_file: copy apps/aroh/.env.release.example there and fill it in"; exit 2; }
set -a
. "$env_file"
set +a
missing=""
for name in AROH_TEAM_ID AROH_DEFAULT_SUPABASE_URL AROH_DEFAULT_SUPABASE_ANON_KEY AROH_DEFAULT_SERVER_URL; do
  eval "value=\${$name:-}"
  [ -n "$value" ] || missing="$missing $name"
done
[ -z "$missing" ] || { echo "set in $env_file:$missing"; exit 2; }
exec sh "$here/ios-release.sh" testflight
