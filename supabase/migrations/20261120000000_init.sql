create sequence public.sync_server_seq;

create table public.goals (
  user_id uuid not null default auth.uid() references auth.users on delete cascade,
  id text not null,
  title text,
  kind text,
  source_url text,
  target_date text,
  status text,
  answers_json text,
  created_at text,
  updated_at text collate "C" not null,
  deleted integer not null default 0,
  server_seq bigint not null default 0,
  primary key (user_id, id)
);

create table public.actions (
  user_id uuid not null default auth.uid() references auth.users on delete cascade,
  id text not null,
  goal_id text,
  title text,
  notes text,
  start_at text,
  end_at text,
  est_minutes integer,
  status text,
  seq integer,
  done_at text,
  updated_at text collate "C" not null,
  deleted integer not null default 0,
  server_seq bigint not null default 0,
  primary key (user_id, id)
);

create table public.plans (
  user_id uuid not null default auth.uid() references auth.users on delete cascade,
  id text not null,
  goal_id text,
  version integer,
  status text,
  summary text,
  updated_at text collate "C" not null,
  deleted integer not null default 0,
  server_seq bigint not null default 0,
  primary key (user_id, id)
);

create table public.replans (
  user_id uuid not null default auth.uid() references auth.users on delete cascade,
  id text not null,
  trigger_action_id text,
  diff_json text,
  size text,
  status text,
  created_at text,
  updated_at text collate "C" not null,
  deleted integer not null default 0,
  server_seq bigint not null default 0,
  primary key (user_id, id)
);

create table public.tracker_entries (
  user_id uuid not null default auth.uid() references auth.users on delete cascade,
  id text not null,
  kind text,
  amount real,
  unit text,
  note text,
  raw_text text,
  logged_at text,
  updated_at text collate "C" not null,
  deleted integer not null default 0,
  server_seq bigint not null default 0,
  primary key (user_id, id)
);

create table public.plugins (
  user_id uuid not null default auth.uid() references auth.users on delete cascade,
  id text not null,
  name text,
  yaml text,
  enabled integer,
  updated_at text collate "C" not null,
  deleted integer not null default 0,
  server_seq bigint not null default 0,
  primary key (user_id, id)
);

create table public.settings (
  user_id uuid not null default auth.uid() references auth.users on delete cascade,
  key text not null,
  value text,
  updated_at text collate "C" not null,
  server_seq bigint not null default 0,
  primary key (user_id, key)
);

create function public.sync_lww() returns trigger
language plpgsql
set search_path = ''
as $$
declare
  new_deleted integer := coalesce((to_jsonb(new) ->> 'deleted')::integer, 0);
  old_deleted integer;
begin
  if tg_op = 'UPDATE' then
    old_deleted := coalesce((to_jsonb(old) ->> 'deleted')::integer, 0);
    if new.updated_at < old.updated_at then
      return null;
    end if;
    if new.updated_at = old.updated_at and not (new_deleted = 1 and old_deleted = 0) then
      return null;
    end if;
  end if;
  new.server_seq := nextval('public.sync_server_seq');
  return new;
end;
$$;

do $$
declare
  t text;
begin
  foreach t in array array['goals', 'actions', 'plans', 'replans', 'tracker_entries', 'plugins', 'settings'] loop
    execute format('create trigger %I before insert or update on public.%I for each row execute function public.sync_lww()', t || '_lww', t);
    execute format('create index %I on public.%I (user_id, server_seq)', t || '_server_seq', t);
    execute format('alter table public.%I enable row level security', t);
    execute format('create policy %I on public.%I for select to authenticated using (user_id = auth.uid())', t || '_select', t);
    execute format('create policy %I on public.%I for insert to authenticated with check (user_id = auth.uid())', t || '_insert', t);
    execute format('create policy %I on public.%I for update to authenticated using (user_id = auth.uid()) with check (user_id = auth.uid())', t || '_update', t);
    execute format('create policy %I on public.%I for delete to authenticated using (user_id = auth.uid())', t || '_delete', t);
    execute format('revoke all on public.%I from public, anon', t);
    execute format('grant select, insert, update, delete on public.%I to authenticated', t);
  end loop;
end;
$$;

revoke all on sequence public.sync_server_seq from public, anon;
grant usage on sequence public.sync_server_seq to authenticated;
revoke all on function public.sync_lww() from public, anon;
