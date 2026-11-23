do $$
begin
  if (select public from storage.buckets where id = 'goal-files') is distinct from false then
    raise exception 'goal-files must be a private bucket';
  end if;
end;
$$;

set role authenticated;
set request.jwt.claim.sub = '00000000-0000-0000-0000-00000000000a';
insert into storage.objects (bucket_id, name)
  values ('goal-files', '00000000-0000-0000-0000-00000000000a/goals/0190f2a0-0000-7000-8000-000000000091/plan.pdf');

do $$
begin
  if (select count(*) from storage.objects) <> 1 then
    raise exception 'A must read its own goal file';
  end if;
  begin
    insert into storage.objects (bucket_id, name)
      values ('goal-files', '00000000-0000-0000-0000-00000000000b/goals/0190f2a0-0000-7000-8000-000000000091/x.pdf');
    raise exception 'A must not write into B''s folder';
  exception when insufficient_privilege then null;
  end;
end;
$$;

set request.jwt.claim.sub = '00000000-0000-0000-0000-00000000000b';
do $$
begin
  if (select count(*) from storage.objects) <> 0 then
    raise exception 'B must not read A''s goal files';
  end if;
  update storage.objects set name = '00000000-0000-0000-0000-00000000000b/stolen.pdf';
  delete from storage.objects;
end;
$$;

set request.jwt.claim.sub = '00000000-0000-0000-0000-00000000000a';
do $$
begin
  if (select name from storage.objects) <> '00000000-0000-0000-0000-00000000000a/goals/0190f2a0-0000-7000-8000-000000000091/plan.pdf' then
    raise exception 'B must not change or delete A''s goal file';
  end if;
end;
$$;

reset role;
