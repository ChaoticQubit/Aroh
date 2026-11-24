#!/bin/sh
set -eu
run() { psql -q -v ON_ERROR_STOP=1 -U postgres -d "$POSTGRES_DB" -f "$1"; }
run /aroh/supabase/tests/stub_auth.sql
for migration in /aroh/supabase/migrations/*.sql; do
  run "$migration"
done
