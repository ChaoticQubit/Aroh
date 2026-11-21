#!/bin/sh
set -eu
crate="$(cd "$(dirname "$0")" && pwd)"
secret() { HOME="${AROH_KEYCHAIN_HOME:-$HOME}" security find-generic-password -s aroh -a "$1" -w; }
if [ -z "${AROH_SUPABASE_URL:-}" ] || [ -z "${AROH_SUPABASE_ANON_KEY:-}" ]; then
  config="$(secret supabase-config)"
  AROH_SUPABASE_URL="$(printf '%s' "$config" | python3 -c 'import json,sys; print(json.load(sys.stdin)["url"])')"
  AROH_SUPABASE_ANON_KEY="$(printf '%s' "$config" | python3 -c 'import json,sys; print(json.load(sys.stdin)["anon_key"])')"
  unset config
fi
login="$(secret supabase-test-login)"
AROH_TEST_EMAIL="${login%%:*}"
AROH_TEST_PASSWORD="${login#*:}"
unset login
export AROH_SUPABASE_URL AROH_SUPABASE_ANON_KEY AROH_TEST_EMAIL AROH_TEST_PASSWORD
cargo test --manifest-path "$crate/Cargo.toml" --test live -- --ignored
