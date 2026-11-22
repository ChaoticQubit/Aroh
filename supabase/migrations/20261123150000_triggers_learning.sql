create table public.action_events (
  id bigint generated always as identity primary key,
  user_id uuid not null references auth.users on delete cascade,
  goal_id uuid not null,
  action_id uuid not null,
  kind text not null check (kind in ('move', 'resize', 'delete', 'missed', 'skipped', 'done')),
  before jsonb,
  after jsonb not null,
  at timestamptz not null default now(),
  handled boolean not null default false,
  handled_by uuid
);
create unique index action_events_status_once on public.action_events (user_id, action_id, kind) where kind in ('missed', 'skipped');
create index action_events_pending on public.action_events (user_id, goal_id) where not handled;

create table public.adaptations (
  id uuid primary key,
  user_id uuid not null references auth.users on delete cascade,
  goal_id uuid,
  source text not null check (source in ('actions', 'rule', 'review', 'edits')),
  window_key text not null check (char_length(window_key) between 1 and 200),
  reason text not null check (char_length(reason) between 1 and 4000),
  run_id uuid references public.runs on delete set null,
  error text,
  created_at timestamptz not null,
  unique nulls not distinct (user_id, goal_id, window_key)
);
create index adaptations_user on public.adaptations (user_id, created_at desc);

create table public.review_schedules (
  user_id uuid not null references auth.users on delete cascade,
  goal_id uuid not null,
  weekday smallint not null check (weekday between 0 and 6),
  at text not null check (at ~ '^([01][0-9]|2[0-3]):[0-5][0-9]$'),
  timezone text not null check (char_length(timezone) between 1 and 64),
  enabled boolean not null,
  next_at timestamptz not null,
  primary key (user_id, goal_id)
);
create index review_schedules_due on public.review_schedules (next_at) where enabled;

do $$
declare
  t text;
begin
  foreach t in array array['action_events', 'adaptations', 'review_schedules'] loop
    execute format('alter table public.%I enable row level security', t);
    execute format('revoke all on public.%I from public, anon, authenticated', t);
    execute format('grant all on public.%I to service_role', t);
  end loop;
end;
$$;

create function public.aroh_action_events() returns trigger
language plpgsql
security definer
set search_path = ''
as $$
declare
  kinds text[] := array[]::text[];
  k text;
  before_row jsonb;
  after_row jsonb;
begin
  if right(new.updated_at, 16) = '00000000000000e5' then
    return null;
  end if;
  if new.deleted and not old.deleted then
    if new.locked then
      kinds := kinds || 'delete'::text;
    end if;
  elsif (new.start_at, new.end_at) is distinct from (old.start_at, old.end_at) and new.locked then
    if new.end_at - new.start_at = old.end_at - old.start_at then
      kinds := kinds || 'move'::text;
    else
      kinds := kinds || 'resize'::text;
    end if;
  end if;
  if new.status is distinct from old.status and new.status in ('missed', 'skipped', 'done') and not new.deleted then
    kinds := kinds || new.status;
  end if;
  before_row := jsonb_build_object('title', old.title, 'start_at', old.start_at, 'end_at', old.end_at, 'status', old.status, 'done_at', old.done_at);
  after_row := jsonb_build_object('title', new.title, 'start_at', new.start_at, 'end_at', new.end_at, 'status', new.status, 'done_at', new.done_at);
  foreach k in array kinds loop
    insert into public.action_events (user_id, goal_id, action_id, kind, before, after)
    values (new.user_id, new.goal_id, new.id, k, before_row, after_row)
    on conflict (user_id, action_id, kind) where kind in ('missed', 'skipped') do nothing;
  end loop;
  return null;
end;
$$;
revoke all on function public.aroh_action_events() from public, anon, authenticated;
create trigger actions_owner_events after update on public.actions for each row execute function public.aroh_action_events();

do $$
begin
  if exists (select 1 from pg_available_extensions where name = 'pg_cron')
    and exists (select 1 from pg_available_extensions where name = 'pg_net')
    and exists (select 1 from pg_namespace where nspname = 'vault') then
    create extension if not exists pg_cron;
    create extension if not exists pg_net;
    perform cron.schedule('aroh-tick', '* * * * *', $job$
      select net.http_post(
        url := (select decrypted_secret from vault.decrypted_secrets where name = 'aroh_engine_url') || '/v1/tick',
        headers := jsonb_build_object(
          'Content-Type', 'application/json',
          'Authorization', 'Bearer ' || (select decrypted_secret from vault.decrypted_secrets where name = 'aroh_service_jwt')
        ),
        body := '{}'::jsonb,
        timeout_milliseconds := 290000
      )
    $job$);
  end if;
exception when others then
  raise notice 'aroh-tick not scheduled: %', sqlerrm;
end;
$$;
