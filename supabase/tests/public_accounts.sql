insert into auth.users (id, email) values
  ('00000000-0000-0000-0000-00000000000c', 'c@example.com'),
  ('00000000-0000-0000-0000-00000000000d', 'd@example.com');

set role authenticated;
set request.jwt.claim.sub = '00000000-0000-0000-0000-00000000000c';

do $$
declare
  i integer;
begin
  for i in 1..5 loop
    insert into public.goals (id, user_id, title, status, autonomy, created_at, updated_at)
      values (('0190f2a0-0000-7000-8000-0000000001' || lpad(i::text, 2, '0'))::uuid, '00000000-0000-0000-0000-00000000000c',
        'Goal ' || i, case when i = 5 then 'draft' else 'active' end, 'balanced', now(), '1763683200001-0000-cccccccccccccccc');
  end loop;
  insert into public.goals (id, user_id, title, status, autonomy, created_at, updated_at)
    values ('0190f2a0-0000-7000-8000-000000000110', '00000000-0000-0000-0000-00000000000c', 'Old', 'archived', 'balanced', now(), '1763683200001-0000-cccccccccccccccc');
  begin
    insert into public.goals (id, user_id, title, status, autonomy, created_at, updated_at)
      values ('0190f2a0-0000-7000-8000-000000000106', '00000000-0000-0000-0000-00000000000c', 'Sixth', 'paused', 'balanced', now(), '1763683200001-0000-cccccccccccccccc');
    raise exception 'a sixth open goal must be refused';
  exception when raise_exception then
    if sqlerrm not like 'goal_limit:%' then
      raise;
    end if;
  end;
  begin
    update public.goals set status = 'active', updated_at = '1763683200002-0000-cccccccccccccccc' where id = '0190f2a0-0000-7000-8000-000000000110';
    raise exception 'un-archiving past the cap must be refused';
  exception when raise_exception then
    if sqlerrm not like 'goal_limit:%' then
      raise;
    end if;
  end;
  update public.goals set title = 'Renamed', updated_at = '1763683200002-0000-cccccccccccccccc' where id = '0190f2a0-0000-7000-8000-000000000101';
  insert into public.goals (id, user_id, title, status, autonomy, created_at, updated_at)
    values ('0190f2a0-0000-7000-8000-000000000102', '00000000-0000-0000-0000-00000000000c', 'Upserted', 'active', 'balanced', now(), '1763683200003-0000-cccccccccccccccc')
    on conflict (user_id, id) do update set title = excluded.title, updated_at = excluded.updated_at;
  update public.goals set status = 'archived', updated_at = '1763683200004-0000-cccccccccccccccc' where id = '0190f2a0-0000-7000-8000-000000000105';
  update public.goals set status = 'active', updated_at = '1763683200004-0000-cccccccccccccccc' where id = '0190f2a0-0000-7000-8000-000000000110';
  if (select count(*) from public.goals where not deleted and status in ('draft', 'active', 'paused')) <> 5 then
    raise exception 'C must hold exactly 5 open goals';
  end if;
  insert into public.goals (id, user_id, title, status, autonomy, created_at, updated_at)
    values ('0190f2a0-0000-7000-8000-000000000105', '00000000-0000-0000-0000-00000000000c', 'Stale', 'active', 'balanced', now(), '1763683200003-0000-cccccccccccccccc')
    on conflict (user_id, id) do update set status = excluded.status, title = excluded.title, updated_at = excluded.updated_at;
  if (select status from public.goals where id = '0190f2a0-0000-7000-8000-000000000105') <> 'archived' then
    raise exception 'a stale push must neither fail nor reopen a goal';
  end if;
  insert into public.goals (id, user_id, title, status, autonomy, created_at, updated_at)
    values ('00000000-0000-7d9f-866c-860554389e92', '00000000-0000-0000-0000-00000000000c', 'Routines', 'active', 'ask', now(), '1763683200001-0000-cccccccccccccccc');
end;
$$;

do $$
declare
  i integer;
begin
  begin
    insert into public.goal_files (id, user_id, goal_id, name, kind, storage_path, size_bytes, ingest_status, updated_at)
      values (gen_random_uuid(), '00000000-0000-0000-0000-00000000000c', '0190f2a0-0000-7000-8000-000000000101', 'big.pdf', 'document',
        'goals/0190f2a0-0000-7000-8000-000000000101/big.pdf', 10485761, 'pending', '1763683200001-0000-cccccccccccccccc');
    raise exception 'a file over 10 MB must be refused';
  exception when raise_exception then
    if sqlerrm not like 'file_too_large:%' then
      raise;
    end if;
  end;
  for i in 1..5 loop
    insert into public.goal_files (id, user_id, goal_id, name, kind, storage_path, size_bytes, ingest_status, updated_at)
      values (gen_random_uuid(), '00000000-0000-0000-0000-00000000000c', '0190f2a0-0000-7000-8000-000000000101', i || '.pdf', 'document',
        'goals/0190f2a0-0000-7000-8000-000000000101/' || i || '.pdf', 10485760, 'pending', '1763683200001-0000-cccccccccccccccc');
  end loop;
  insert into public.goal_files (id, user_id, goal_id, name, kind, url, size_bytes, ingest_status, updated_at)
    values (gen_random_uuid(), '00000000-0000-0000-0000-00000000000c', '0190f2a0-0000-7000-8000-000000000101', 'link', 'link',
      'https://example.org', 0, 'done', '1763683200001-0000-cccccccccccccccc');
  begin
    insert into public.goal_files (id, user_id, goal_id, name, kind, storage_path, size_bytes, ingest_status, updated_at)
      values (gen_random_uuid(), '00000000-0000-0000-0000-00000000000c', '0190f2a0-0000-7000-8000-000000000101', 'x.pdf', 'document',
        'goals/0190f2a0-0000-7000-8000-000000000101/x.pdf', 1, 'pending', '1763683200001-0000-cccccccccccccccc');
    raise exception 'files past 50 MB must be refused';
  exception when raise_exception then
    if sqlerrm not like 'storage_full:%' then
      raise;
    end if;
  end;
  update public.goal_files set ingest_status = 'done', updated_at = '1763683200002-0000-cccccccccccccccc' where name = '1.pdf';
end;
$$;

do $$
declare
  i integer;
begin
  for i in 1..20 loop
    insert into storage.objects (bucket_id, name)
      values ('goal-files', '00000000-0000-0000-0000-00000000000c/goals/0190f2a0-0000-7000-8000-000000000101/o' || i || '.pdf');
  end loop;
  begin
    insert into storage.objects (bucket_id, name)
      values ('goal-files', '00000000-0000-0000-0000-00000000000c/goals/0190f2a0-0000-7000-8000-000000000101/o21.pdf');
    raise exception 'a 21st stored file must be refused';
  exception when insufficient_privilege then null;
  end;
  begin
    perform public.aroh_delete_account('00000000-0000-0000-0000-00000000000d');
    raise exception 'users must not call the delete function';
  exception when insufficient_privilege then null;
  end;
  begin
    perform count(*) from public.ai_consents;
    raise exception 'users must not read consents';
  exception when insufficient_privilege then null;
  end;
  begin
    perform public.aroh_capacity();
    raise exception 'users must not read capacity';
  exception when insufficient_privilege then null;
  end;
end;
$$;

reset role;

do $$
begin
  if (select file_size_limit from storage.buckets where id = 'goal-files') <> 10485760 then
    raise exception 'goal-files must cap a file at 10 MB';
  end if;
end;
$$;

insert into public.ai_consents (user_id, granted_at) values ('00000000-0000-0000-0000-00000000000c', now()), ('00000000-0000-0000-0000-00000000000d', now());
insert into public.run_limits (user_id, minutes, updated_at) values ('00000000-0000-0000-0000-00000000000c', 30, now());
insert into public.devices (id, user_id, platform, token, environment, zone, synced_seq, scheduled_until, created_at, updated_at)
  values (gen_random_uuid(), '00000000-0000-0000-0000-00000000000c', 'ios', repeat('c7', 32), 'sandbox', 'UTC', 0, now(), now(), now());
insert into public.goals (id, user_id, title, status, autonomy, created_at, updated_at)
  values ('0190f2a0-0000-7000-8000-000000000101', '00000000-0000-0000-0000-00000000000d', 'D goal', 'active', 'balanced', now(), '1763683200001-0000-dddddddddddddddd');

do $$
declare
  t text;
  n bigint;
begin
  begin
    perform public.aroh_delete_account('00000000-0000-0000-0000-00000000000c');
    raise exception 'deletion must wait for the stored files';
  exception when raise_exception then
    if sqlerrm not like 'files_remain:%' then
      raise;
    end if;
  end;
  delete from storage.objects where name like '00000000-0000-0000-0000-00000000000c/%';
  perform public.aroh_delete_account('00000000-0000-0000-0000-00000000000c');
  perform public.aroh_delete_account('00000000-0000-0000-0000-00000000000c');
  for t in
    select c.table_name from information_schema.columns c
      join information_schema.tables tb on tb.table_schema = c.table_schema and tb.table_name = c.table_name
      where c.table_schema = 'public' and c.column_name = 'user_id' and tb.table_type = 'BASE TABLE'
  loop
    execute format('select count(*) from public.%I where user_id = $1', t) into n using '00000000-0000-0000-0000-00000000000c'::uuid;
    if n <> 0 then
      raise exception 'deleted account still has % rows in %', n, t;
    end if;
  end loop;
  if exists (select 1 from auth.users where id = '00000000-0000-0000-0000-00000000000c') then
    raise exception 'the auth user must be gone';
  end if;
  if not exists (select 1 from public.goals where user_id = '00000000-0000-0000-0000-00000000000d')
     or not exists (select 1 from public.ai_consents where user_id = '00000000-0000-0000-0000-00000000000d') then
    raise exception 'another account must be untouched';
  end if;
end;
$$;

do $$
declare
  i integer;
  answer jsonb;
begin
  update public.signup_settings set paused = true;
  answer := public.aroh_before_user_created('{"metadata": {"ip_address": "198.51.100.7"}, "user": {}}');
  if answer->'error'->>'message' not like 'signups_paused:%' or (answer->'error'->>'http_code')::int <> 403 then
    raise exception 'a paused switch must refuse sign-ups: %', answer;
  end if;
  update public.signup_settings set paused = false, max_users = 1;
  if (select signups_open from public.aroh_capacity()) then
    raise exception 'capacity past max_users must close sign-ups';
  end if;
  answer := public.aroh_before_user_created('{"metadata": {"ip_address": "198.51.100.7"}, "user": {}}');
  if answer->'error'->>'message' not like 'signups_paused:%' then
    raise exception 'sign-ups past capacity must be refused: %', answer;
  end if;
  update public.signup_settings set max_users = 40000;
  for i in 1..5 loop
    answer := public.aroh_before_user_created('{"metadata": {"ip_address": "198.51.100.7"}, "user": {}}');
    if answer <> '{}'::jsonb then
      raise exception 'sign-up % must pass: %', i, answer;
    end if;
  end loop;
  answer := public.aroh_before_user_created('{"metadata": {"ip_address": "198.51.100.7"}, "user": {}}');
  if (answer->'error'->>'http_code')::int <> 429 or answer->'error'->>'message' not like 'signup_rate_limited:%' then
    raise exception 'a sixth sign-up from one address in an hour must be refused: %', answer;
  end if;
  if public.aroh_before_user_created('{"metadata": {"ip_address": "198.51.100.8"}, "user": {}}') <> '{}'::jsonb
     or public.aroh_before_user_created('{"metadata": {}, "user": {}}') <> '{}'::jsonb then
    raise exception 'other addresses must still sign up';
  end if;
  delete from public.signup_attempts;
end;
$$;

insert into auth.users (id, email) values ('00000000-0000-0000-0000-00000000000e', 'e@example.com');
insert into public.goals (id, user_id, title, status, autonomy, created_at, updated_at)
  values ('0190f2a0-0000-7000-8000-000000000101', '00000000-0000-0000-0000-00000000000e', 'E goal', 'active', 'balanced', now(), '1763683200001-0000-eeeeeeeeeeeeeeee');
insert into public.devices (id, user_id, platform, token, environment, zone, synced_seq, scheduled_until, created_at, updated_at)
  values (gen_random_uuid(), '00000000-0000-0000-0000-00000000000e', 'ios', repeat('e7', 32), 'sandbox', 'UTC', 0, now(), now(), now());
insert into public.jobs (user_id, kind) values ('00000000-0000-0000-0000-00000000000e', 'run');
insert into auth.sessions (id, user_id) values ('0190f2a0-0000-7000-8000-0000000000e1', '00000000-0000-0000-0000-00000000000e');
insert into auth.refresh_tokens (user_id, session_id) values ('00000000-0000-0000-0000-00000000000e', '0190f2a0-0000-7000-8000-0000000000e1');

do $$
declare
  due timestamptz;
begin
  due := public.aroh_request_account_deletion('00000000-0000-0000-0000-00000000000e', '2026-11-23T10:00:00Z', interval '15 days');
  if due <> '2026-12-08T10:00:00Z'::timestamptz then
    raise exception 'deletion must be due 15 days later, got %', due;
  end if;
  if public.aroh_request_account_deletion('00000000-0000-0000-0000-00000000000e', '2026-11-25T10:00:00Z', interval '15 days') <> due then
    raise exception 'a repeated request keeps the first date';
  end if;
  if exists (select 1 from public.devices where user_id = '00000000-0000-0000-0000-00000000000e')
     or exists (select 1 from auth.sessions where user_id = '00000000-0000-0000-0000-00000000000e')
     or exists (select 1 from auth.refresh_tokens where user_id = '00000000-0000-0000-0000-00000000000e') then
    raise exception 'a disabled account keeps no device or session';
  end if;
  if not exists (select 1 from public.jobs where user_id = '00000000-0000-0000-0000-00000000000e' and status = 'queued') then
    raise exception 'queued jobs wait for the grace period to end';
  end if;
end;
$$;

set role authenticated;
set request.jwt.claim.sub = '00000000-0000-0000-0000-00000000000e';
do $$
begin
  if (select count(*) from public.goals) <> 0 then
    raise exception 'a disabled account must not read its rows';
  end if;
  begin
    insert into public.goals (id, user_id, title, status, autonomy, created_at, updated_at)
      values (gen_random_uuid(), '00000000-0000-0000-0000-00000000000e', 'New', 'active', 'balanced', now(), '1763683200009-0000-eeeeeeeeeeeeeeee');
    raise exception 'a disabled account must not write';
  exception when insufficient_privilege then null;
  end;
end;
$$;
reset role;

update auth.users set last_sign_in_at = '2026-11-24T09:00:00Z' where id = '00000000-0000-0000-0000-00000000000e';

set role authenticated;
set request.jwt.claim.sub = '00000000-0000-0000-0000-00000000000e';
do $$
begin
  if (select count(*) from public.goals) <> 1 then
    raise exception 'signing back in must restore the account';
  end if;
end;
$$;
reset role;

do $$
begin
  if exists (select 1 from public.account_deletions where user_id = '00000000-0000-0000-0000-00000000000e') then
    raise exception 'signing back in must cancel the deletion';
  end if;
end;
$$;
