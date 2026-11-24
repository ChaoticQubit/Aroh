#!/bin/sh
set -eu
psql -v ON_ERROR_STOP=1 -U postgres -d postgres -v pw="$POSTGRES_PASSWORD" <<'SQL'
create role anon nologin noinherit;
create role authenticated nologin noinherit;
create role service_role nologin noinherit bypassrls;
create role authenticator login noinherit password :'pw';
grant anon, authenticated, service_role to authenticator;
create role supabase_auth_admin login noinherit createrole password :'pw';
create schema auth authorization supabase_auth_admin;
alter role supabase_auth_admin set search_path = auth;
create schema extensions;
grant usage on schema auth to anon, authenticated, service_role;
grant usage on schema public to anon, authenticated, service_role;
grant usage on schema extensions to anon, authenticated, service_role;
create schema _realtime;
create publication supabase_realtime;
create schema realtime;
create schema aroh_admin;
create table aroh_admin.migrations (name text primary key, applied_at timestamptz not null default now());
SQL
