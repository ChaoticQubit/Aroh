do $$
begin
  if not exists (select 1 from public.ai_consents where user_id = '00000000-0000-0000-0000-0000000000f1') then
    raise exception 'an account with a goal gets consent';
  end if;
  if exists (select 1 from public.ai_consents where user_id = '00000000-0000-0000-0000-0000000000f2') then
    raise exception 'the seeded Routines goal alone is no consent';
  end if;
  if (select granted_at from public.ai_consents where user_id = '00000000-0000-0000-0000-0000000000f3') <> '2026-01-01T00:00:00Z' then
    raise exception 'an existing consent keeps its date';
  end if;
end;
$$;
