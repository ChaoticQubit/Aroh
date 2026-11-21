insert into auth.users (id, email) values
  ('00000000-0000-0000-0000-00000000000a', 'a@example.com'),
  ('00000000-0000-0000-0000-00000000000b', 'b@example.com');

set role authenticated;

set request.jwt.claim.sub = '00000000-0000-0000-0000-00000000000a';
insert into public.goals (id, title, updated_at) values ('g1', 'A goal', '1763683200001-0000-aaaaaaaaaaaaaaaa');
insert into public.settings (key, value, updated_at) values ('theme', 'dark', '1763683200001-0000-aaaaaaaaaaaaaaaa');

set request.jwt.claim.sub = '00000000-0000-0000-0000-00000000000b';
insert into public.goals (id, title, updated_at) values ('g1', 'B goal', '1763683200001-0000-bbbbbbbbbbbbbbbb');
insert into public.goals (id, title, updated_at) values ('g2', 'B goal 2', '1763683200001-0000-bbbbbbbbbbbbbbbb');

do $$
declare
  n integer;
begin
  if (select count(*) from public.goals) <> 2 then
    raise exception 'B must see only its own 2 goals';
  end if;
  begin
    insert into public.goals (user_id, id, title, updated_at)
      values ('00000000-0000-0000-0000-00000000000a', 'g9', 'forged', '1763683200001-0000-bbbbbbbbbbbbbbbb');
    raise exception 'B wrote a row with A user_id';
  exception when insufficient_privilege then
    null;
  end;
  update public.goals set title = 'hijack' where user_id = '00000000-0000-0000-0000-00000000000a';
  get diagnostics n = row_count;
  if n <> 0 then
    raise exception 'B updated A rows';
  end if;
  delete from public.goals where user_id = '00000000-0000-0000-0000-00000000000a';
  get diagnostics n = row_count;
  if n <> 0 then
    raise exception 'B deleted A rows';
  end if;
end;
$$;

set request.jwt.claim.sub = '00000000-0000-0000-0000-00000000000a';

do $$
declare
  seq0 bigint;
  seq1 bigint;
begin
  if (select count(*) from public.goals) <> 1 or (select title from public.goals where id = 'g1') <> 'A goal' then
    raise exception 'A must see only its own untouched goal';
  end if;
  select server_seq into seq0 from public.goals where id = 'g1';

  update public.goals set title = 'older', updated_at = '1763683200000-0000-aaaaaaaaaaaaaaaa' where id = 'g1';
  if (select title from public.goals where id = 'g1') <> 'A goal' then
    raise exception 'older update must be skipped';
  end if;

  insert into public.goals (id, title, updated_at) values ('g1', 'older upsert', '1763683200000-0000-aaaaaaaaaaaaaaaa')
    on conflict (user_id, id) do update set title = excluded.title, updated_at = excluded.updated_at;
  if (select title from public.goals where id = 'g1') <> 'A goal' then
    raise exception 'older upsert must be skipped';
  end if;

  update public.goals set title = 'same stamp' where id = 'g1';
  if (select title from public.goals where id = 'g1') <> 'A goal' then
    raise exception 'equal stamp edit must be skipped';
  end if;

  update public.goals set deleted = 1 where id = 'g1';
  if (select deleted from public.goals where id = 'g1') <> 1 then
    raise exception 'equal stamp tombstone must win';
  end if;

  update public.goals set title = 'newer', deleted = 0, updated_at = '1763683200002-0000-aaaaaaaaaaaaaaaa' where id = 'g1';
  select server_seq into seq1 from public.goals where id = 'g1';
  if (select title from public.goals where id = 'g1') <> 'newer' or seq1 <= seq0 then
    raise exception 'newer update must apply and advance server_seq';
  end if;

  update public.settings set value = 'light' where key = 'theme';
  if (select value from public.settings where key = 'theme') <> 'dark' then
    raise exception 'equal stamp settings edit must be skipped';
  end if;
  update public.settings set value = 'light', updated_at = '1763683200002-0000-aaaaaaaaaaaaaaaa' where key = 'theme';
  if (select value from public.settings where key = 'theme') <> 'light' then
    raise exception 'newer settings edit must apply';
  end if;
end;
$$;

reset role;
set role anon;
reset request.jwt.claim.sub;

do $$
begin
  begin
    perform 1 from public.goals;
    raise exception 'anon read goals';
  exception when insufficient_privilege then
    null;
  end;
  begin
    insert into public.goals (id, updated_at) values ('x', '1763683200001-0000-aaaaaaaaaaaaaaaa');
    raise exception 'anon wrote goals';
  exception when insufficient_privilege then
    null;
  end;
end;
$$;
