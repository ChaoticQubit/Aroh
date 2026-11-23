#!/bin/sh
set -eu
here="$(cd "$(dirname "$0")" && pwd)"
export HOME="${KIRI_HOME:-$HOME}"
name="aroh-rls-$$"
cleanup() { kiri rm -f "$name" >/dev/null 2>&1 || true; }
trap cleanup EXIT INT TERM
kiri run -d --rm --name "$name" -e POSTGRES_PASSWORD=test pgvector/pgvector:pg17 >/dev/null
tries=0
until kiri exec "$name" pg_isready -U postgres -h 127.0.0.1 >/dev/null 2>&1; do
  tries=$((tries + 1))
  if [ "$tries" -gt 60 ]; then
    echo "postgres did not start" >&2
    exit 1
  fi
  sleep 1
done
psql() { kiri exec -i "$name" psql -q -v ON_ERROR_STOP=1 -U postgres -h 127.0.0.1 -d postgres; }
psql < "$here/stub_auth.sql"
for migration in "$here"/../migrations/*.sql; do
  psql < "$migration"
done
psql < "$here/rls_rebuild.sql"
psql < "$here/rls_places.sql"
psql < "$here/stub_storage.sql"
psql < "$here/../migrations/20261127020000_goal_files_bucket.sql"
psql < "$here/rls_storage.sql"
echo "rls: ok"
