#!/bin/sh
set -eu
export PGPASSWORD="$POSTGRES_PASSWORD"
db() { psql -q -v ON_ERROR_STOP=1 -h pg -U postgres -d postgres "$@"; }
until db -tAc "select 1 from auth.users limit 1" >/dev/null 2>&1 && db -tAc "select 1 from storage.buckets limit 1" >/dev/null 2>&1; do
  sleep 1
done
for migration in /aroh/supabase/migrations/*.sql; do
  name="$(basename "$migration")"
  if [ -n "$(db -tAc "select 1 from aroh_admin.migrations where name = '$name'")" ]; then
    continue
  fi
  db -1 -f "$migration" -c "insert into aroh_admin.migrations (name) values ('$name')"
  echo "applied $name"
done
db -c "notify pgrst, 'reload schema'"
echo "aroh migrations up to date"
