#!/bin/sh
set -eu
export PGPASSWORD=postgres
until psql -h pg -U postgres -d postgres -tAc "select 1 from auth.users limit 1" >/dev/null 2>&1; do
  sleep 1
done
for migration in /aroh/supabase/migrations/*.sql; do
  psql -q -v ON_ERROR_STOP=1 -h pg -U postgres -d postgres -f "$migration"
done
psql -q -h pg -U postgres -d postgres -c "notify pgrst, 'reload schema'"
echo "aroh migrations applied"
