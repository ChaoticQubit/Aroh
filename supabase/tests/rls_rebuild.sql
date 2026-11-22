insert into auth.users (id, email) values
  ('00000000-0000-0000-0000-00000000000a', 'a@example.com'),
  ('00000000-0000-0000-0000-00000000000b', 'b@example.com');

insert into public.categories (id, user_id, name, color, seeded, updated_at)
  values (gen_random_uuid(), '00000000-0000-0000-0000-00000000000a', 'Habit', '#7a5a4f', true, '1763683200001-0000-aaaaaaaaaaaaaaaa');
insert into public.goals (id, user_id, title, status, autonomy, created_at, updated_at)
  values ('0190f2a0-0000-7000-8000-000000000091', '00000000-0000-0000-0000-00000000000a', 'A goal', 'active', 'balanced', now(), '1763683200001-0000-aaaaaaaaaaaaaaaa');
insert into public.goal_links (id, user_id, from_goal_id, to_goal_id, kind, confirmed, updated_at)
  values (gen_random_uuid(), '00000000-0000-0000-0000-00000000000a', '0190f2a0-0000-7000-8000-000000000091', gen_random_uuid(), 'supports', false, '1763683200001-0000-aaaaaaaaaaaaaaaa');
insert into public.goal_files (id, user_id, goal_id, name, kind, storage_path, size_bytes, ingest_status, updated_at)
  values (gen_random_uuid(), '00000000-0000-0000-0000-00000000000a', '0190f2a0-0000-7000-8000-000000000091', 'plan.pdf', 'document', 'goals/0190f2a0-0000-7000-8000-000000000091/plan.pdf', 10, 'pending', '1763683200001-0000-aaaaaaaaaaaaaaaa');
insert into public.personas (id, user_id, name, toml, updated_at)
  values (gen_random_uuid(), '00000000-0000-0000-0000-00000000000a', 'coach', 'role = "coach"', '1763683200001-0000-aaaaaaaaaaaaaaaa');
insert into public.plugins (id, user_id, name, version, bundle_toml, enabled, updated_at)
  values (gen_random_uuid(), '00000000-0000-0000-0000-00000000000a', 'fitness', 1, 'name = "fitness"', true, '1763683200001-0000-aaaaaaaaaaaaaaaa');
insert into public.trackers (id, user_id, goal_id, name, fields, rules, updated_at)
  values (gen_random_uuid(), '00000000-0000-0000-0000-00000000000a', '0190f2a0-0000-7000-8000-000000000091', 'Body', '[]', '[]', '1763683200001-0000-aaaaaaaaaaaaaaaa');
insert into public.tracker_entries (id, user_id, tracker_id, data, source, logged_at, updated_at)
  values (gen_random_uuid(), '00000000-0000-0000-0000-00000000000a', gen_random_uuid(), '{"weight": 81}', 'log', now(), '1763683200001-0000-aaaaaaaaaaaaaaaa');
insert into public.action_series (id, user_id, goal_id, title, rrule, dtstart_local, timezone, duration_minutes, updated_at)
  values (gen_random_uuid(), '00000000-0000-0000-0000-00000000000a', '0190f2a0-0000-7000-8000-000000000091', 'Core', 'FREQ=DAILY', '2026-11-23T07:00:00', 'Europe/Berlin', 45, '1763683200001-0000-aaaaaaaaaaaaaaaa');
insert into public.actions (id, user_id, goal_id, title, start_at, end_at, status, locked, updated_at)
  values (gen_random_uuid(), '00000000-0000-0000-0000-00000000000a', '0190f2a0-0000-7000-8000-000000000091', 'Buy a scale', now(), now() + interval '1 hour', 'planned', false, '1763683200001-0000-aaaaaaaaaaaaaaaa');
insert into public.proposals (id, user_id, goal_id, summary, diff, size, status, created_at, updated_at)
  values (gen_random_uuid(), '00000000-0000-0000-0000-00000000000a', '0190f2a0-0000-7000-8000-000000000091', 'Move workout', '[]', 'small', 'proposed', now(), '1763683200001-0000-aaaaaaaaaaaaaaaa');
insert into public.preferences (id, user_id, fact, source, noted_at, confirmed, updated_at)
  values (gen_random_uuid(), '00000000-0000-0000-0000-00000000000a', 'Mornings', 'chat', now(), true, '1763683200001-0000-aaaaaaaaaaaaaaaa');
insert into public.busy_blocks (id, user_id, calendar_id, event_id, title, start_at, end_at, all_day, updated_at)
  values (gen_random_uuid(), '00000000-0000-0000-0000-00000000000a', 'work', 'evt-1', 'Standup', now(), now() + interval '15 minutes', false, '1763683200001-0000-aaaaaaaaaaaaaaaa');
insert into public.calendar_rules (id, user_id, calendar_id, group_name, decision, updated_at)
  values (gen_random_uuid(), '00000000-0000-0000-0000-00000000000a', 'work', 'Work', 'keep', '1763683200001-0000-aaaaaaaaaaaaaaaa');
insert into public.provider_settings (id, user_id, label, vendor, model, is_default, updated_at)
  values ('0190f2a0-0000-7000-8000-000000000016', '00000000-0000-0000-0000-00000000000a', 'Claude', 'anthropic', 'claude-opus', true, '1763683200001-0000-aaaaaaaaaaaaaaaa');
insert into public.search_settings (id, user_id, label, provider, priority, enabled, updated_at)
  values (gen_random_uuid(), '00000000-0000-0000-0000-00000000000a', 'SearXNG', 'searxng', 0, true, '1763683200001-0000-aaaaaaaaaaaaaaaa');
insert into public.budgets (id, user_id, scope, limit_usd_micros, updated_at)
  values (gen_random_uuid(), '00000000-0000-0000-0000-00000000000a', 'user_month', 5000000, '1763683200001-0000-aaaaaaaaaaaaaaaa');
insert into public.usage (id, user_id, vendor, model, input_tokens, output_tokens, cost_usd_micros, occurred_at, updated_at)
  values (gen_random_uuid(), '00000000-0000-0000-0000-00000000000a', 'anthropic', 'claude-opus', 10, 5, 42, now(), '1763683200001-0000-aaaaaaaaaaaaaaaa');
insert into public.jobs (id, user_id, kind)
  values ('0190f2a0-0000-7000-8000-0000000000a1', '00000000-0000-0000-0000-00000000000a', 'setup');
insert into public.runs (id, user_id, job_id, goal_id, workflow, state)
  values ('0190f2a0-0000-7000-8000-0000000000a2', '00000000-0000-0000-0000-00000000000a', '0190f2a0-0000-7000-8000-0000000000a1', '0190f2a0-0000-7000-8000-000000000091', 'setup', 'queued');
insert into public.run_checkpoints (user_id, run_id, step, state)
  values ('00000000-0000-0000-0000-00000000000a', '0190f2a0-0000-7000-8000-0000000000a2', 0, '{}');
insert into public.run_events (user_id, run_id, seq, kind)
  values ('00000000-0000-0000-0000-00000000000a', '0190f2a0-0000-7000-8000-0000000000a2', 0, 'step');
insert into public.knowledge_chunks (user_id, id, goal_id, content, embedding)
  values ('00000000-0000-0000-0000-00000000000a', '0123456789abcdef', '0190f2a0-0000-7000-8000-000000000091', 'protein per meal', array_fill(0.1::real, array[384])::extensions.vector);
insert into public.memory_logs (user_id, log, position, message)
  values ('00000000-0000-0000-0000-00000000000a', 'goal:setup', 0, '{"role": "user"}');
insert into public.provider_credentials (user_id, setting_id, ciphertext, nonce, key_version, vendor, last4)
  values ('00000000-0000-0000-0000-00000000000a', '0190f2a0-0000-7000-8000-000000000016', '\x00ff', '\x01', 1, 'anthropic', 'abcd');
insert into public.user_keys (user_id, wrapped, nonce, key_version)
  values ('00000000-0000-0000-0000-00000000000a', '\x00ff', '\x01', 1);
insert into public.devices (id, user_id, platform, token, environment, zone, synced_seq, scheduled_until, created_at, updated_at)
  values ('0190f2a0-0000-7000-8000-0000000000d1', '00000000-0000-0000-0000-00000000000a', 'ios', repeat('ab', 32), 'sandbox', 'Europe/Berlin', 0, now(), now(), now());
insert into public.push_sent (device_id, key, sent_at)
  values ('0190f2a0-0000-7000-8000-0000000000d1', 'proposal:x', now());

create schema rls_test;
create table rls_test.fixtures (tbl text primary key, cols text not null, forged jsonb not null);
grant usage on schema rls_test to authenticated, anon;
grant select on rls_test.fixtures to authenticated, anon;

do $$
declare
  t text;
  r jsonb;
  n integer;
begin
  select count(*) into n from pg_class c join pg_namespace s on s.oid = c.relnamespace
    where s.nspname = 'public' and c.relkind in ('r', 'p');
  if n <> 28 then
    raise exception 'expected 28 public tables, found %', n;
  end if;
  select count(*) into n from pg_class c join pg_namespace s on s.oid = c.relnamespace
    where s.nspname = 'public' and c.relkind in ('r', 'p') and not c.relrowsecurity;
  if n <> 0 then
    raise exception '% public tables without row level security', n;
  end if;
  for t in select c.relname from pg_class c join pg_namespace s on s.oid = c.relnamespace
      where s.nspname = 'public' and c.relkind = 'r' and exists (select 1 from information_schema.columns
        where table_schema = 'public' and table_name = c.relname and column_name = 'user_id') loop
    execute format('select to_jsonb(x) from public.%I x where user_id = %L', t, '00000000-0000-0000-0000-00000000000a') into r;
    if r is null then
      raise exception 'no fixture row in %', t;
    end if;
    if t = 'knowledge_chunks' then
      r := jsonb_set(r, '{id}', '"fedcba9876543210"');
    elsif r ? 'id' and t <> 'run_events' then
      r := jsonb_set(r, '{id}', to_jsonb(gen_random_uuid()));
    end if;
    insert into rls_test.fixtures values (t,
      (select string_agg(quote_ident(column_name), ', ' order by ordinal_position) from information_schema.columns
        where table_schema = 'public' and table_name = t and is_generated = 'NEVER' and is_identity = 'NO'),
      r);
  end loop;
end;
$$;

set role authenticated;
set request.jwt.claim.sub = '00000000-0000-0000-0000-00000000000b';

do $$
declare
  f record;
  n bigint;
begin
  for f in select * from rls_test.fixtures loop
    begin
      execute format('select count(*) from public.%I', f.tbl) into n;
      if n <> 0 then
        raise exception 'B read A rows of %', f.tbl;
      end if;
    exception when insufficient_privilege then
      null;
    end;
    begin
      if f.forged ? 'server_seq' then
        execute format('update public.%I set updated_at = %L', f.tbl, '9000000000000-0000-bbbbbbbbbbbbbbbb');
      else
        execute format('update public.%I set user_id = user_id', f.tbl);
      end if;
      get diagnostics n = row_count;
      if n <> 0 then
        raise exception 'B updated A rows of %', f.tbl;
      end if;
    exception when insufficient_privilege then
      null;
    end;
    begin
      execute format('delete from public.%I', f.tbl);
      get diagnostics n = row_count;
      if n <> 0 then
        raise exception 'B deleted A rows of %', f.tbl;
      end if;
    exception when insufficient_privilege then
      null;
    end;
    begin
      execute format('insert into public.%I (%s) select %s from jsonb_populate_record(null::public.%I, %L)',
        f.tbl, f.cols, f.cols, f.tbl, f.forged);
      raise exception 'B forged a row with A user_id in %', f.tbl;
    exception when insufficient_privilege then
      null;
    end;
  end loop;

  insert into public.goals (id, title, status, autonomy, created_at, updated_at)
    values ('0190f2a0-0000-7000-8000-0000000000b1', 'B goal', 'active', 'ask', now(), '1763683200001-0000-bbbbbbbbbbbbbbbb');
  begin
    update public.goals set user_id = '00000000-0000-0000-0000-00000000000a', updated_at = '1763683200009-0000-bbbbbbbbbbbbbbbb'
      where id = '0190f2a0-0000-7000-8000-0000000000b1';
    raise exception 'B moved its row to A';
  exception when insufficient_privilege then
    null;
  end;
  if (select count(*) from public.goals) <> 1 then
    raise exception 'B must see exactly its own goal';
  end if;
end;
$$;

set request.jwt.claim.sub = '00000000-0000-0000-0000-00000000000a';

do $$
declare
  t text;
  n bigint;
  seq0 bigint;
  seq1 bigint;
begin
  foreach t in array array['categories', 'goals', 'goal_links', 'goal_files', 'personas', 'plugins', 'trackers',
    'tracker_entries', 'action_series', 'actions', 'proposals', 'preferences', 'busy_blocks', 'calendar_rules',
    'provider_settings', 'search_settings', 'budgets', 'usage', 'jobs', 'runs', 'run_events'] loop
    execute format('select count(*) from public.%I', t) into n;
    if n <> 1 then
      raise exception 'A must see its one row of %, saw %', t, n;
    end if;
  end loop;

  foreach t in array array['provider_credentials', 'user_keys', 'run_checkpoints', 'knowledge_chunks', 'memory_logs', 'devices', 'push_sent'] loop
    begin
      execute format('select count(*) from public.%I', t) into n;
      raise exception 'A read server-only table %', t;
    exception when insufficient_privilege then
      null;
    end;
  end loop;

  begin
    insert into public.usage (id, user_id, vendor, model, input_tokens, output_tokens, cost_usd_micros, occurred_at, updated_at)
      values (gen_random_uuid(), '00000000-0000-0000-0000-00000000000a', 'x', 'y', 0, 0, 0, now(), '1763683200002-0000-aaaaaaaaaaaaaaaa');
    raise exception 'A wrote usage';
  exception when insufficient_privilege then
    null;
  end;
  begin
    update public.usage set cost_usd_micros = 0;
    raise exception 'A rewrote usage';
  exception when insufficient_privilege then
    null;
  end;
  begin
    insert into public.jobs (user_id, kind) values ('00000000-0000-0000-0000-00000000000a', 'free');
    raise exception 'A enqueued a job directly';
  exception when insufficient_privilege then
    null;
  end;

  begin
    insert into public.goals (id, title, status, autonomy, created_at, updated_at)
      values (gen_random_uuid(), 'bad stamp', 'active', 'ask', now(), 'yesterday');
    raise exception 'malformed hlc accepted';
  exception when check_violation then
    null;
  end;
  begin
    insert into public.goals (id, title, status, autonomy, created_at, updated_at)
      values (gen_random_uuid(), 'bad status', 'exploded', 'ask', now(), '1763683200002-0000-aaaaaaaaaaaaaaaa');
    raise exception 'unknown status accepted';
  exception when check_violation then
    null;
  end;

  select server_seq into seq0 from public.goals where id = '0190f2a0-0000-7000-8000-000000000091';

  update public.goals set title = 'older', updated_at = '1763683200000-0000-aaaaaaaaaaaaaaaa'
    where id = '0190f2a0-0000-7000-8000-000000000091';
  if (select title from public.goals) <> 'A goal' then
    raise exception 'older update must be skipped';
  end if;

  insert into public.goals (id, title, status, autonomy, created_at, updated_at)
    values ('0190f2a0-0000-7000-8000-000000000091', 'older upsert', 'active', 'ask', now(), '1763683200000-0000-aaaaaaaaaaaaaaaa')
    on conflict (user_id, id) do update set title = excluded.title, updated_at = excluded.updated_at;
  if (select title from public.goals) <> 'A goal' then
    raise exception 'older upsert must be skipped';
  end if;

  update public.goals set title = 'same stamp' where id = '0190f2a0-0000-7000-8000-000000000091';
  if (select title from public.goals) <> 'A goal' then
    raise exception 'equal stamp edit must be skipped';
  end if;

  update public.goals set deleted = true where id = '0190f2a0-0000-7000-8000-000000000091';
  if not (select deleted from public.goals) then
    raise exception 'equal stamp tombstone must win';
  end if;

  update public.goals set title = 'newer', deleted = false, updated_at = '1763683200002-0000-aaaaaaaaaaaaaaaa'
    where id = '0190f2a0-0000-7000-8000-000000000091';
  select server_seq into seq1 from public.goals;
  if (select title from public.goals) <> 'newer' or seq1 <= seq0 then
    raise exception 'newer update must apply and advance server_seq';
  end if;
end;
$$;

reset role;
set role anon;
reset request.jwt.claim.sub;

do $$
declare
  f record;
begin
  for f in select * from rls_test.fixtures loop
    begin
      execute format('select 1 from public.%I limit 1', f.tbl);
      raise exception 'anon read %', f.tbl;
    exception when insufficient_privilege then
      null;
    end;
  end loop;
end;
$$;

reset role;
set role service_role;

do $$
begin
  if (select count(*) from public.provider_credentials) <> 1 then
    raise exception 'service_role must read provider_credentials';
  end if;
end;
$$;

reset role;
