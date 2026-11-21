drop table if exists public.goals, public.actions, public.plans, public.replans, public.tracker_entries, public.plugins, public.settings;
drop function if exists public.sync_lww();
drop sequence if exists public.sync_server_seq;

create extension if not exists vector with schema extensions;

create sequence public.aroh_sync_seq;

create table public.categories (
  id uuid not null,
  user_id uuid not null default auth.uid() references auth.users on delete cascade,
  name text not null,
  color text not null check (color ~ '^#[0-9a-fA-F]{6}$'),
  seeded boolean not null,
  updated_at text collate "C" not null,
  deleted boolean not null default false,
  server_seq bigint not null default 0,
  primary key (user_id, id)
);

create table public.goals (
  id uuid not null,
  user_id uuid not null default auth.uid() references auth.users on delete cascade,
  category_id uuid,
  title text not null,
  context text,
  deadline date,
  status text not null check (status in ('draft', 'active', 'paused', 'done', 'archived')),
  autonomy text not null check (autonomy in ('ask', 'balanced', 'auto')),
  created_at timestamptz not null,
  updated_at text collate "C" not null,
  deleted boolean not null default false,
  server_seq bigint not null default 0,
  primary key (user_id, id)
);

create table public.goal_links (
  id uuid not null,
  user_id uuid not null default auth.uid() references auth.users on delete cascade,
  from_goal_id uuid not null,
  to_goal_id uuid not null,
  kind text not null check (kind in ('parent', 'depends_on', 'supports', 'conflicts')),
  confirmed boolean not null,
  updated_at text collate "C" not null,
  deleted boolean not null default false,
  server_seq bigint not null default 0,
  primary key (user_id, id),
  check (from_goal_id <> to_goal_id)
);

create table public.goal_files (
  id uuid not null,
  user_id uuid not null default auth.uid() references auth.users on delete cascade,
  goal_id uuid not null,
  name text not null,
  kind text not null check (kind in ('document', 'image', 'audio', 'video', 'link')),
  storage_path text,
  url text,
  mime text,
  size_bytes bigint not null check (size_bytes >= 0),
  ingest_status text not null check (ingest_status in ('pending', 'processing', 'done', 'failed')),
  updated_at text collate "C" not null,
  deleted boolean not null default false,
  server_seq bigint not null default 0,
  primary key (user_id, id),
  check ((kind = 'link') = (url is not null and storage_path is null))
);

create table public.personas (
  id uuid not null,
  user_id uuid not null default auth.uid() references auth.users on delete cascade,
  goal_id uuid,
  name text not null,
  toml text not null,
  updated_at text collate "C" not null,
  deleted boolean not null default false,
  server_seq bigint not null default 0,
  primary key (user_id, id)
);

create table public.plugins (
  id uuid not null,
  user_id uuid not null default auth.uid() references auth.users on delete cascade,
  goal_id uuid,
  name text not null,
  version bigint not null check (version >= 1),
  bundle_toml text not null,
  enabled boolean not null,
  updated_at text collate "C" not null,
  deleted boolean not null default false,
  server_seq bigint not null default 0,
  primary key (user_id, id)
);

create table public.trackers (
  id uuid not null,
  user_id uuid not null default auth.uid() references auth.users on delete cascade,
  goal_id uuid not null,
  plugin_id uuid,
  name text not null,
  fields jsonb not null check (jsonb_typeof(fields) = 'array'),
  cadence jsonb,
  rules jsonb not null check (jsonb_typeof(rules) = 'array'),
  updated_at text collate "C" not null,
  deleted boolean not null default false,
  server_seq bigint not null default 0,
  primary key (user_id, id)
);

create table public.tracker_entries (
  id uuid not null,
  user_id uuid not null default auth.uid() references auth.users on delete cascade,
  tracker_id uuid not null,
  data jsonb not null check (jsonb_typeof(data) = 'object'),
  source text not null check (source in ('check_in', 'log', 'health')),
  raw_text text,
  logged_at timestamptz not null,
  updated_at text collate "C" not null,
  deleted boolean not null default false,
  server_seq bigint not null default 0,
  primary key (user_id, id)
);

create table public.action_series (
  id uuid not null,
  user_id uuid not null default auth.uid() references auth.users on delete cascade,
  goal_id uuid not null,
  title text not null,
  notes text,
  rrule text not null,
  dtstart_local text not null,
  timezone text not null,
  duration_minutes bigint not null check (duration_minutes between 1 and 1440),
  updated_at text collate "C" not null,
  deleted boolean not null default false,
  server_seq bigint not null default 0,
  primary key (user_id, id)
);

create table public.actions (
  id uuid not null,
  user_id uuid not null default auth.uid() references auth.users on delete cascade,
  goal_id uuid not null,
  series_id uuid,
  occurrence timestamptz,
  title text not null,
  notes text,
  start_at timestamptz not null,
  end_at timestamptz not null,
  status text not null check (status in ('planned', 'done', 'missed', 'skipped')),
  locked boolean not null,
  done_at timestamptz,
  updated_at text collate "C" not null,
  deleted boolean not null default false,
  server_seq bigint not null default 0,
  primary key (user_id, id),
  check (end_at > start_at),
  check ((series_id is null) = (occurrence is null))
);

create table public.proposals (
  id uuid not null,
  user_id uuid not null default auth.uid() references auth.users on delete cascade,
  goal_id uuid not null,
  run_id uuid,
  summary text not null,
  diff jsonb not null,
  size text not null check (size in ('small', 'big')),
  status text not null check (status in ('proposed', 'applied', 'rejected', 'undone')),
  undo jsonb,
  created_at timestamptz not null,
  updated_at text collate "C" not null,
  deleted boolean not null default false,
  server_seq bigint not null default 0,
  primary key (user_id, id)
);

create table public.preferences (
  id uuid not null,
  user_id uuid not null default auth.uid() references auth.users on delete cascade,
  fact text not null,
  source text not null,
  noted_at timestamptz not null,
  confirmed boolean not null,
  updated_at text collate "C" not null,
  deleted boolean not null default false,
  server_seq bigint not null default 0,
  primary key (user_id, id)
);

create table public.busy_blocks (
  id uuid not null,
  user_id uuid not null default auth.uid() references auth.users on delete cascade,
  calendar_id text not null,
  event_id text not null,
  title text not null,
  start_at timestamptz not null,
  end_at timestamptz not null,
  all_day boolean not null,
  group_name text,
  updated_at text collate "C" not null,
  deleted boolean not null default false,
  server_seq bigint not null default 0,
  primary key (user_id, id),
  check (end_at > start_at)
);

create table public.calendar_rules (
  id uuid not null,
  user_id uuid not null default auth.uid() references auth.users on delete cascade,
  calendar_id text,
  series_key text,
  pattern text,
  group_name text not null,
  decision text not null check (decision in ('keep', 'ignore')),
  updated_at text collate "C" not null,
  deleted boolean not null default false,
  server_seq bigint not null default 0,
  primary key (user_id, id),
  check (num_nonnulls(calendar_id, series_key, pattern) > 0)
);

create table public.provider_settings (
  id uuid not null,
  user_id uuid not null default auth.uid() references auth.users on delete cascade,
  label text not null,
  vendor text not null,
  model text not null,
  base_url text,
  is_default boolean not null,
  updated_at text collate "C" not null,
  deleted boolean not null default false,
  server_seq bigint not null default 0,
  primary key (user_id, id)
);

create table public.search_settings (
  id uuid not null,
  user_id uuid not null default auth.uid() references auth.users on delete cascade,
  label text not null,
  provider text not null,
  base_url text,
  priority bigint not null check (priority between 0 and 1000),
  enabled boolean not null,
  updated_at text collate "C" not null,
  deleted boolean not null default false,
  server_seq bigint not null default 0,
  primary key (user_id, id)
);

create table public.budgets (
  id uuid not null,
  user_id uuid not null default auth.uid() references auth.users on delete cascade,
  scope text not null check (scope in ('call', 'run', 'goal_month', 'user_month')),
  goal_id uuid,
  limit_usd_micros bigint not null check (limit_usd_micros > 0),
  updated_at text collate "C" not null,
  deleted boolean not null default false,
  server_seq bigint not null default 0,
  primary key (user_id, id),
  check ((scope = 'goal_month') = (goal_id is not null))
);

create table public.usage (
  id uuid not null,
  user_id uuid not null default auth.uid() references auth.users on delete cascade,
  goal_id uuid,
  run_id uuid,
  vendor text not null,
  model text not null,
  input_tokens bigint not null check (input_tokens >= 0),
  output_tokens bigint not null check (output_tokens >= 0),
  cost_usd_micros bigint not null check (cost_usd_micros >= 0),
  occurred_at timestamptz not null,
  updated_at text collate "C" not null,
  deleted boolean not null default false,
  server_seq bigint not null default 0,
  primary key (user_id, id)
);

create function public.aroh_lww() returns trigger
language plpgsql
set search_path = ''
as $$
begin
  perform pg_catalog.pg_advisory_xact_lock(pg_catalog.hashtextextended(new.user_id::text, 0));
  if tg_op = 'UPDATE' then
    if new.updated_at < old.updated_at then
      return null;
    end if;
    if new.updated_at = old.updated_at and not (new.deleted and not old.deleted) then
      return null;
    end if;
  end if;
  new.server_seq := pg_catalog.nextval('public.aroh_sync_seq');
  return new;
end;
$$;

create table public.jobs (
  id uuid primary key default gen_random_uuid(),
  user_id uuid not null references auth.users on delete cascade,
  kind text not null,
  payload jsonb not null default '{}',
  status text not null default 'queued' check (status in ('queued', 'running', 'done', 'failed')),
  run_after timestamptz not null default now(),
  attempts integer not null default 0 check (attempts >= 0),
  locked_until timestamptz,
  created_at timestamptz not null default now(),
  updated_at timestamptz not null default now()
);
create index jobs_due on public.jobs (run_after) where status = 'queued';
create index jobs_user on public.jobs (user_id, created_at);

create table public.runs (
  id uuid primary key,
  user_id uuid not null references auth.users on delete cascade,
  job_id uuid references public.jobs on delete set null,
  goal_id uuid,
  workflow text not null,
  state text not null check (state in ('queued', 'running', 'checkpointed', 'waiting_human', 'waiting_budget', 'done', 'failed')),
  created_at timestamptz not null default now(),
  updated_at timestamptz not null default now()
);
create index runs_user on public.runs (user_id, created_at);

create table public.run_checkpoints (
  user_id uuid not null references auth.users on delete cascade,
  run_id uuid not null references public.runs on delete cascade,
  step bigint not null check (step >= 0),
  state jsonb not null,
  created_at timestamptz not null default now(),
  primary key (run_id, step)
);

create table public.run_events (
  id bigint generated always as identity primary key,
  user_id uuid not null references auth.users on delete cascade,
  run_id uuid not null references public.runs on delete cascade,
  seq bigint not null check (seq >= 0),
  kind text not null,
  payload jsonb not null default '{}',
  created_at timestamptz not null default now(),
  unique (run_id, seq)
);
create index run_events_user on public.run_events (user_id, run_id);

create table public.knowledge_chunks (
  user_id uuid not null references auth.users on delete cascade,
  id text not null check (id ~ '^[0-9a-f]{16}$'),
  goal_id uuid,
  content text not null,
  meta jsonb not null default '{}',
  embedding extensions.vector(384) not null,
  created_at timestamptz not null default now(),
  tsv tsvector generated always as (to_tsvector('simple', content)) stored,
  primary key (user_id, id)
);
create index knowledge_chunks_embedding on public.knowledge_chunks using hnsw (embedding extensions.vector_cosine_ops);
create index knowledge_chunks_tsv on public.knowledge_chunks using gin (tsv);
create index knowledge_chunks_goal on public.knowledge_chunks (user_id, goal_id);

create table public.memory_logs (
  user_id uuid not null references auth.users on delete cascade,
  log text not null,
  position bigint not null check (position >= 0),
  message jsonb not null,
  created_at timestamptz not null default now(),
  primary key (user_id, log, position)
);

create table public.provider_credentials (
  id uuid primary key default gen_random_uuid(),
  user_id uuid not null references auth.users on delete cascade,
  setting_id uuid not null,
  ciphertext bytea not null,
  nonce bytea not null,
  key_version integer not null check (key_version >= 1),
  created_at timestamptz not null default now(),
  updated_at timestamptz not null default now(),
  unique (user_id, setting_id)
);

do $$
declare
  t text;
begin
  foreach t in array array['categories', 'goals', 'goal_links', 'goal_files', 'personas', 'plugins', 'trackers',
    'tracker_entries', 'action_series', 'actions', 'proposals', 'preferences', 'busy_blocks', 'calendar_rules',
    'provider_settings', 'search_settings', 'budgets', 'usage'] loop
    execute format('alter table public.%I add constraint %I check (updated_at ~ ''^[0-9]{13}-[0-9a-f]{4}-[0-9a-f]{16}$'')', t, t || '_hlc');
    execute format('create trigger %I before insert or update on public.%I for each row execute function public.aroh_lww()', t || '_lww', t);
    execute format('create index %I on public.%I (user_id, server_seq)', t || '_server_seq', t);
    execute format('alter table public.%I enable row level security', t);
    execute format('revoke all on public.%I from public, anon, authenticated', t);
    execute format('grant all on public.%I to service_role', t);
    execute format('create policy %I on public.%I for select to authenticated using (user_id = (select auth.uid()))', t || '_select', t);
    if t = 'usage' then
      execute format('grant select on public.%I to authenticated', t);
    else
      execute format('create policy %I on public.%I for insert to authenticated with check (user_id = (select auth.uid()))', t || '_insert', t);
      execute format('create policy %I on public.%I for update to authenticated using (user_id = (select auth.uid())) with check (user_id = (select auth.uid()))', t || '_update', t);
      execute format('create policy %I on public.%I for delete to authenticated using (user_id = (select auth.uid()))', t || '_delete', t);
      execute format('grant select, insert, update, delete on public.%I to authenticated', t);
    end if;
  end loop;

  foreach t in array array['jobs', 'runs', 'run_checkpoints', 'run_events', 'knowledge_chunks', 'memory_logs', 'provider_credentials'] loop
    execute format('alter table public.%I enable row level security', t);
    execute format('revoke all on public.%I from public, anon, authenticated', t);
    execute format('grant all on public.%I to service_role', t);
    if t in ('jobs', 'runs', 'run_events') then
      execute format('create policy %I on public.%I for select to authenticated using (user_id = (select auth.uid()))', t || '_select', t);
      execute format('grant select on public.%I to authenticated', t);
    end if;
  end loop;

  if exists (select 1 from pg_publication where pubname = 'supabase_realtime') then
    alter publication supabase_realtime add table public.run_events;
  end if;
end;
$$;

revoke all on sequence public.aroh_sync_seq from public, anon;
grant usage on sequence public.aroh_sync_seq to authenticated, service_role;
revoke all on function public.aroh_lww() from public, anon, authenticated;
