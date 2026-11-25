set role authenticated;
set request.jwt.claim.sub = '00000000-0000-0000-0000-00000000000a';

insert into public.diary_entries (id, day, at, key_version, nonce, ciphertext, updated_at)
  values ('0190f2a0-0000-7000-8000-0000000000d1', '2026-11-26', '2026-11-26T07:45:00Z', 1, 'AAAAAAAAAAAAAAAA', 'c2VhbGVkIGVudHJ5', '1763683200005-0000-aaaaaaaaaaaaaaaa');
insert into public.diary_days (id, day, key_version, nonce, ciphertext, updated_at)
  values ('5a0f2a0e-0000-5000-8000-0000000000d2', '2026-11-26', 1, 'AAAAAAAAAAAAAAAA', 'c2VhbGVkIGNhcmQh', '1763683200005-0000-aaaaaaaaaaaaaaaa');

do $$
begin
  if (select count(*) from public.diary_entries where id = '0190f2a0-0000-7000-8000-0000000000d1') <> 1 or (select count(*) from public.diary_days where id = '5a0f2a0e-0000-5000-8000-0000000000d2') <> 1 then
    raise exception 'A must read its own diary rows';
  end if;
  if (select user_id from public.diary_entries where id = '0190f2a0-0000-7000-8000-0000000000d1') <> '00000000-0000-0000-0000-00000000000a' then
    raise exception 'a diary entry defaults to the caller';
  end if;
  update public.diary_days set ciphertext = 'bmV3ZXIgY2FyZCE=', updated_at = '1763683200006-0000-aaaaaaaaaaaaaaaa' where id = '5a0f2a0e-0000-5000-8000-0000000000d2';
  update public.diary_days set ciphertext = 'b2xkZXIgY2FyZCE=', updated_at = '1763683200001-0000-aaaaaaaaaaaaaaaa' where id = '5a0f2a0e-0000-5000-8000-0000000000d2';
  if (select ciphertext from public.diary_days where id = '5a0f2a0e-0000-5000-8000-0000000000d2') <> 'bmV3ZXIgY2FyZCE=' then
    raise exception 'an older card must lose to a newer one';
  end if;
  begin
    insert into public.diary_entries (id, day, at, key_version, nonce, ciphertext, updated_at)
      values (gen_random_uuid(), '2026-11-26', now(), 1, 'AAAAAAAAAAAAAAAA', 'plain words here!', '1763683200007-0000-aaaaaaaaaaaaaaaa');
    raise exception 'a non-base64 ciphertext was accepted';
  exception when check_violation then
    null;
  end;
  begin
    insert into public.diary_entries (id, day, at, key_version, nonce, ciphertext, updated_at)
      values (gen_random_uuid(), '2026-11-26', now(), 0, 'AAAAAAAAAAAAAAAA', 'c2VhbGVk', '1763683200007-0000-aaaaaaaaaaaaaaaa');
    raise exception 'key version 0 was accepted';
  exception when check_violation then
    null;
  end;
  begin
    insert into public.diary_days (id, day, key_version, nonce, ciphertext, updated_at)
      values (gen_random_uuid(), '2026-11-27', 1, 'AAAA', 'c2VhbGVk', '1763683200007-0000-aaaaaaaaaaaaaaaa');
    raise exception 'a short nonce was accepted';
  exception when check_violation then
    null;
  end;
  begin
    perform count(*) from public.diary_keys;
    raise exception 'an account read diary_keys';
  exception when insufficient_privilege then
    null;
  end;
end;
$$;

set request.jwt.claim.sub = '00000000-0000-0000-0000-00000000000b';

do $$
declare
  n bigint;
begin
  if (select count(*) from public.diary_entries) <> 0 or (select count(*) from public.diary_days) <> 0 then
    raise exception 'B read A diary rows';
  end if;
  update public.diary_days set ciphertext = 'c3RvbGVuIGNhcmQh', updated_at = '9000000000000-0000-bbbbbbbbbbbbbbbb';
  get diagnostics n = row_count;
  if n <> 0 then
    raise exception 'B changed A diary card';
  end if;
  delete from public.diary_entries;
  get diagnostics n = row_count;
  if n <> 0 then
    raise exception 'B deleted A diary entries';
  end if;
  begin
    insert into public.diary_entries (id, user_id, day, at, key_version, nonce, ciphertext, updated_at)
      values (gen_random_uuid(), '00000000-0000-0000-0000-00000000000a', '2026-11-26', now(), 1, 'AAAAAAAAAAAAAAAA', 'Zm9yZ2Vk', '1763683200008-0000-bbbbbbbbbbbbbbbb');
    raise exception 'B forged a diary entry for A';
  exception when insufficient_privilege then
    null;
  end;
  begin
    insert into public.diary_keys (user_id, key_version, sealed, nonce)
      values ('00000000-0000-0000-0000-00000000000b', 1, '\x00', '\x000000000000000000000000');
    raise exception 'an account wrote diary_keys';
  exception when insufficient_privilege then
    null;
  end;
end;
$$;

reset role;
reset request.jwt.claim.sub;

do $$
begin
  if (select count(*) from public.diary_entries where id = '0190f2a0-0000-7000-8000-0000000000d1') <> 1 or (select count(*) from public.diary_days where id = '5a0f2a0e-0000-5000-8000-0000000000d2') <> 1 then
    raise exception 'A diary rows must survive B';
  end if;
  if (select ciphertext from public.diary_days where id = '5a0f2a0e-0000-5000-8000-0000000000d2') <> 'bmV3ZXIgY2FyZCE=' then
    raise exception 'B changed A card';
  end if;
end;
$$;

delete from public.diary_entries where id = '0190f2a0-0000-7000-8000-0000000000d1';
delete from public.diary_days where id = '5a0f2a0e-0000-5000-8000-0000000000d2';
