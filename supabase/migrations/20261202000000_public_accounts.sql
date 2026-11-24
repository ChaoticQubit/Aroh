create table public.ai_consents (
  user_id uuid primary key references auth.users on delete cascade,
  granted_at timestamptz not null
);
alter table public.ai_consents enable row level security;
revoke all on public.ai_consents from public, anon, authenticated;
grant all on public.ai_consents to service_role;

create function public.aroh_goal_quota() returns trigger
language plpgsql
security definer
set search_path = ''
as $$
declare
  open_count bigint;
begin
  if new.deleted or new.status not in ('draft', 'active', 'paused') or new.id = '00000000-0000-7d9f-866c-860554389e92' then
    return new;
  end if;
  perform pg_catalog.pg_advisory_xact_lock(pg_catalog.hashtextextended(new.user_id::text, 0));
  if exists (select 1 from public.goals g where g.user_id = new.user_id and g.id = new.id
             and ((not g.deleted and g.status in ('draft', 'active', 'paused')) or g.updated_at >= new.updated_at)) then
    return new;
  end if;
  select count(*) into open_count from public.goals g
    where g.user_id = new.user_id and g.id <> new.id and not g.deleted and g.status in ('draft', 'active', 'paused')
      and g.id <> '00000000-0000-7d9f-866c-860554389e92';
  if open_count >= 5 then
    raise exception using errcode = 'P0001', message = 'goal_limit: You have 5 active goals. Archive one to add another.';
  end if;
  return new;
end;
$$;
revoke all on function public.aroh_goal_quota() from public, anon, authenticated;
create trigger goals_quota before insert or update on public.goals for each row execute function public.aroh_goal_quota();

create function public.aroh_goal_file_quota() returns trigger
language plpgsql
security definer
set search_path = ''
as $$
declare
  used bigint;
begin
  if new.deleted or new.storage_path is null then
    return new;
  end if;
  perform pg_catalog.pg_advisory_xact_lock(pg_catalog.hashtextextended(new.user_id::text, 0));
  if exists (select 1 from public.goal_files f where f.user_id = new.user_id and f.id = new.id
             and ((not f.deleted and f.storage_path is not null and f.size_bytes >= new.size_bytes) or f.updated_at >= new.updated_at)) then
    return new;
  end if;
  if new.size_bytes > 10485760 then
    raise exception using errcode = 'P0001', message = 'file_too_large: A file can be at most 10 MB.';
  end if;
  select coalesce(sum(f.size_bytes), 0) into used from public.goal_files f
    where f.user_id = new.user_id and f.id <> new.id and not f.deleted and f.storage_path is not null;
  if used + new.size_bytes > 52428800 then
    raise exception using errcode = 'P0001', message = 'storage_full: Your files use the 50 MB allowed. Delete a file to add another.';
  end if;
  return new;
end;
$$;
revoke all on function public.aroh_goal_file_quota() from public, anon, authenticated;
create trigger goal_files_quota before insert or update on public.goal_files for each row execute function public.aroh_goal_file_quota();

create function public.aroh_delete_account(target uuid) returns void
language plpgsql
security definer
set search_path = ''
as $$
declare
  t text;
begin
  if pg_catalog.to_regclass('storage.objects') is not null then
    if exists (select 1 from storage.objects o where o.bucket_id = 'goal-files' and (storage.foldername(o.name))[1] = target::text) then
      raise exception using errcode = 'P0001', message = 'files_remain: The account still has stored files. Try again.';
    end if;
  end if;
  perform pg_catalog.pg_advisory_xact_lock(pg_catalog.hashtextextended(target::text, 0));
  for t in
    select c.table_name from information_schema.columns c
      join information_schema.tables tb on tb.table_schema = c.table_schema and tb.table_name = c.table_name
      where c.table_schema = 'public' and c.column_name = 'user_id' and tb.table_type = 'BASE TABLE'
      order by c.table_name
  loop
    execute pg_catalog.format('delete from public.%I where user_id = $1', t) using target;
  end loop;
  delete from auth.users where id = target;
end;
$$;
revoke all on function public.aroh_delete_account(uuid) from public, anon, authenticated;
grant execute on function public.aroh_delete_account(uuid) to service_role;

create table public.account_deletions (
  user_id uuid primary key references auth.users on delete cascade,
  requested_at timestamptz not null,
  purge_after timestamptz not null check (purge_after > requested_at)
);
create index account_deletions_due on public.account_deletions (purge_after);
alter table public.account_deletions enable row level security;
revoke all on public.account_deletions from public, anon, authenticated;
grant all on public.account_deletions to service_role;

create function public.aroh_account_disabled() returns boolean
language sql
stable
security definer
set search_path = ''
as $$
  select exists (select 1 from public.account_deletions d where d.user_id = (select auth.uid()))
$$;
revoke all on function public.aroh_account_disabled() from public, anon;
grant execute on function public.aroh_account_disabled() to authenticated, service_role;

create function public.aroh_request_account_deletion(target uuid, at timestamptz, grace interval) returns timestamptz
language plpgsql
security definer
set search_path = ''
as $$
declare
  due timestamptz;
begin
  insert into public.account_deletions (user_id, requested_at, purge_after) values (target, at, at + grace)
    on conflict (user_id) do nothing;
  delete from public.devices where user_id = target;
  if pg_catalog.to_regclass('auth.refresh_tokens') is not null then
    execute 'delete from auth.refresh_tokens where user_id = $1' using target::text;
  end if;
  if pg_catalog.to_regclass('auth.sessions') is not null then
    execute 'delete from auth.sessions where user_id = $1' using target;
  end if;
  select d.purge_after into strict due from public.account_deletions d where d.user_id = target;
  return due;
end;
$$;
revoke all on function public.aroh_request_account_deletion(uuid, timestamptz, interval) from public, anon, authenticated;
grant execute on function public.aroh_request_account_deletion(uuid, timestamptz, interval) to service_role;

create function public.aroh_cancel_deletion_on_sign_in() returns trigger
language plpgsql
security definer
set search_path = ''
as $$
begin
  if new.last_sign_in_at is not null and new.last_sign_in_at is distinct from old.last_sign_in_at then
    delete from public.account_deletions d where d.user_id = new.id and d.requested_at < new.last_sign_in_at;
  end if;
  return new;
end;
$$;
revoke all on function public.aroh_cancel_deletion_on_sign_in() from public, anon, authenticated;

do $$
begin
  if exists (select 1 from information_schema.columns
             where table_schema = 'auth' and table_name = 'users' and column_name = 'last_sign_in_at') then
    create trigger aroh_cancel_deletion_on_sign_in after update of last_sign_in_at on auth.users
      for each row execute function public.aroh_cancel_deletion_on_sign_in();
  else
    raise notice 'auth.users has no last_sign_in_at, sign-in cancel trigger skipped';
  end if;
end;
$$;

create table public.signup_settings (
  id boolean primary key default true check (id),
  paused boolean not null default false,
  max_users bigint not null check (max_users >= 0),
  max_db_bytes bigint not null check (max_db_bytes >= 0),
  max_storage_bytes bigint not null check (max_storage_bytes >= 0),
  signups_per_ip_hour integer not null check (signups_per_ip_hour >= 0),
  updated_at timestamptz not null default now()
);
insert into public.signup_settings (max_users, max_db_bytes, max_storage_bytes, signups_per_ip_hour)
  values (40000, 419430400, 838860800, 5);
alter table public.signup_settings enable row level security;
revoke all on public.signup_settings from public, anon, authenticated;
grant all on public.signup_settings to service_role;

create table public.signup_attempts (
  id bigint generated always as identity primary key,
  ip text not null check (char_length(ip) between 1 and 64),
  at timestamptz not null default now()
);
create index signup_attempts_ip on public.signup_attempts (ip, at);
alter table public.signup_attempts enable row level security;
revoke all on public.signup_attempts from public, anon, authenticated;
grant all on public.signup_attempts to service_role;

create function public.aroh_capacity()
returns table (users bigint, db_bytes bigint, storage_bytes bigint, max_users bigint, max_db_bytes bigint,
  max_storage_bytes bigint, paused boolean, signups_open boolean)
language plpgsql
security definer
set search_path = ''
as $$
declare
  s public.signup_settings;
begin
  select * into strict s from public.signup_settings;
  users := (select count(*) from auth.users);
  db_bytes := pg_catalog.pg_database_size(pg_catalog.current_database());
  storage_bytes := 0;
  if exists (select 1 from information_schema.columns
             where table_schema = 'storage' and table_name = 'objects' and column_name = 'metadata') then
    execute 'select coalesce(sum((metadata->>''size'')::bigint), 0) from storage.objects' into storage_bytes;
  end if;
  max_users := s.max_users;
  max_db_bytes := s.max_db_bytes;
  max_storage_bytes := s.max_storage_bytes;
  paused := s.paused;
  signups_open := not s.paused and users < s.max_users and db_bytes < s.max_db_bytes and storage_bytes < s.max_storage_bytes;
  return next;
end;
$$;
revoke all on function public.aroh_capacity() from public, anon, authenticated;
grant execute on function public.aroh_capacity() to service_role;

create function public.aroh_before_user_created(event jsonb) returns jsonb
language plpgsql
security definer
set search_path = ''
as $$
declare
  client_ip text := pg_catalog.left(event->'metadata'->>'ip_address', 64);
  per_hour integer;
  recent bigint;
begin
  if not (select c.signups_open from public.aroh_capacity() c) then
    return pg_catalog.jsonb_build_object('error', pg_catalog.jsonb_build_object('http_code', 403,
      'message', 'signups_paused: New sign-ups are paused for now. Please try again later.'));
  end if;
  if client_ip is null or client_ip = '' then
    return '{}'::jsonb;
  end if;
  select s.signups_per_ip_hour into per_hour from public.signup_settings s;
  perform pg_catalog.pg_advisory_xact_lock(pg_catalog.hashtextextended('signup:' || client_ip, 0));
  delete from public.signup_attempts a where a.at < pg_catalog.now() - interval '1 day';
  select count(*) into recent from public.signup_attempts a where a.ip = client_ip and a.at > pg_catalog.now() - interval '1 hour';
  if recent >= per_hour then
    return pg_catalog.jsonb_build_object('error', pg_catalog.jsonb_build_object('http_code', 429,
      'message', 'signup_rate_limited: Too many sign-ups from this network. Please try again in an hour.'));
  end if;
  insert into public.signup_attempts (ip) values (client_ip);
  return '{}'::jsonb;
end;
$$;
revoke all on function public.aroh_before_user_created(jsonb) from public, anon, authenticated;

do $$
begin
  if exists (select 1 from pg_roles where rolname = 'supabase_auth_admin') then
    grant execute on function public.aroh_before_user_created(jsonb) to supabase_auth_admin;
  end if;
end;
$$;

do $$
declare
  t text;
begin
  for t in
    select c.relname from pg_class c join pg_namespace s on s.oid = c.relnamespace
      where s.nspname = 'public' and c.relkind = 'r' and c.relrowsecurity
        and exists (select 1 from information_schema.columns
                    where table_schema = 'public' and table_name = c.relname and column_name = 'user_id')
      order by c.relname
  loop
    execute format('create policy %I on public.%I as restrictive for all to authenticated
      using (not (select public.aroh_account_disabled())) with check (not (select public.aroh_account_disabled()))', t || '_enabled', t);
  end loop;
end;
$$;
