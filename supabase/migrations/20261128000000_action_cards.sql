alter table public.action_series
  add column card jsonb check (card is null or jsonb_typeof(card) = 'object');

alter table public.actions
  add column card jsonb check (card is null or jsonb_typeof(card) = 'object');

create table public.action_logs (
  id uuid not null,
  user_id uuid not null default auth.uid() references auth.users on delete cascade,
  action_id uuid not null,
  goal_id uuid not null,
  sets jsonb not null check (jsonb_typeof(sets) = 'array'),
  checked jsonb not null check (jsonb_typeof(checked) = 'array'),
  effort text check (effort in ('easy', 'ok', 'hard')),
  note text check (char_length(note) <= 4000),
  logged_at timestamptz not null,
  updated_at text collate "C" not null,
  deleted boolean not null default false,
  server_seq bigint not null default 0,
  primary key (user_id, id)
);
create index action_logs_goal on public.action_logs (user_id, goal_id, logged_at);

do $$
declare
  t text;
begin
  foreach t in array array['action_logs'] loop
    execute format('alter table public.%I add constraint %I check (updated_at ~ ''^[0-9]{13}-[0-9a-f]{4}-[0-9a-f]{16}$'')', t, t || '_hlc');
    execute format('create trigger %I before insert or update on public.%I for each row execute function public.aroh_lww()', t || '_lww', t);
    execute format('create index %I on public.%I (user_id, server_seq)', t || '_server_seq', t);
    execute format('alter table public.%I enable row level security', t);
    execute format('revoke all on public.%I from public, anon, authenticated', t);
    execute format('grant all on public.%I to service_role', t);
    execute format('create policy %I on public.%I for select to authenticated using (user_id = (select auth.uid()))', t || '_select', t);
    execute format('create policy %I on public.%I for insert to authenticated with check (user_id = (select auth.uid()))', t || '_insert', t);
    execute format('create policy %I on public.%I for update to authenticated using (user_id = (select auth.uid())) with check (user_id = (select auth.uid()))', t || '_update', t);
    execute format('create policy %I on public.%I for delete to authenticated using (user_id = (select auth.uid()))', t || '_delete', t);
    execute format('grant select, insert, update, delete on public.%I to authenticated', t);
  end loop;
end;
$$;
