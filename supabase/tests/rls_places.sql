do $$
declare
  n integer;
begin
  select count(*) into n from information_schema.tables
    where table_name in ('visits', 'place_candidates', 'pattern_candidates', 'nudge_sent');
  if n <> 0 then
    raise exception 'raw location tables must not exist on the server, found %', n;
  end if;
  select count(*) into n from information_schema.columns
    where table_schema = 'public' and table_name = 'places' and column_name in ('arrived_at', 'left_at', 'accuracy_m', 'visit_count');
  if n <> 0 then
    raise exception 'places must not carry visit columns';
  end if;
end;
$$;

set role authenticated;
set request.jwt.claim.sub = '00000000-0000-0000-0000-00000000000a';

insert into public.places (id, name, lat_e6, lon_e6, radius_m, source, updated_at)
  values ('0190f2a0-0000-7000-8000-0000000000e1', 'Iron Works Gym', 37776000, -122417000, 150, 'prompt', '1763683200005-0000-aaaaaaaaaaaaaaaa');
insert into public.place_links (id, place_id, goal_id, series_id, updated_at)
  values ('0190f2a0-0000-7000-8000-0000000000e2', '0190f2a0-0000-7000-8000-0000000000e1', '0190f2a0-0000-7000-8000-000000000091', null, '1763683200005-0000-aaaaaaaaaaaaaaaa');

do $$
begin
  if (select count(*) from public.places where id = '0190f2a0-0000-7000-8000-0000000000e1') <> 1 then
    raise exception 'A must read its own place';
  end if;
  if (select user_id from public.places where id = '0190f2a0-0000-7000-8000-0000000000e1') <> '00000000-0000-0000-0000-00000000000a' then
    raise exception 'a place defaults to the caller';
  end if;
  update public.places set name = 'Gym', updated_at = '1763683200006-0000-aaaaaaaaaaaaaaaa' where id = '0190f2a0-0000-7000-8000-0000000000e1';
  if (select name from public.places where id = '0190f2a0-0000-7000-8000-0000000000e1') <> 'Gym' then
    raise exception 'A must edit its own place';
  end if;
  update public.places set name = 'Older', updated_at = '1763683200001-0000-aaaaaaaaaaaaaaaa' where id = '0190f2a0-0000-7000-8000-0000000000e1';
  if (select name from public.places where id = '0190f2a0-0000-7000-8000-0000000000e1') <> 'Gym' then
    raise exception 'an older place edit must lose';
  end if;
  if (select count(*) from public.place_links where place_id = '0190f2a0-0000-7000-8000-0000000000e1') <> 1 then
    raise exception 'A must read its own link';
  end if;
  begin
    insert into public.places (id, name, lat_e6, lon_e6, radius_m, source, updated_at)
      values (gen_random_uuid(), 'Moon', 95000000, 0, 150, 'prompt', '1763683200007-0000-aaaaaaaaaaaaaaaa');
    raise exception 'a place off the globe was accepted';
  exception when check_violation then
    null;
  end;
end;
$$;

set request.jwt.claim.sub = '00000000-0000-0000-0000-00000000000b';

do $$
declare
  n bigint;
begin
  if (select count(*) from public.places) <> 0 or (select count(*) from public.place_links) <> 0 then
    raise exception 'B read A places or links';
  end if;
  update public.places set name = 'Stolen', updated_at = '9000000000000-0000-bbbbbbbbbbbbbbbb';
  get diagnostics n = row_count;
  if n <> 0 then
    raise exception 'B renamed A places';
  end if;
  delete from public.place_links;
  get diagnostics n = row_count;
  if n <> 0 then
    raise exception 'B deleted A links';
  end if;
  begin
    insert into public.places (id, user_id, name, lat_e6, lon_e6, radius_m, source, updated_at)
      values (gen_random_uuid(), '00000000-0000-0000-0000-00000000000a', 'Forged', 1, 1, 150, 'search', '1763683200008-0000-bbbbbbbbbbbbbbbb');
    raise exception 'B forged a place for A';
  exception when insufficient_privilege then
    null;
  end;
end;
$$;

reset role;
reset request.jwt.claim.sub;
