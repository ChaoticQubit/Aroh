insert into public.ai_consents (user_id, granted_at)
  select distinct g.user_id, now() from public.goals g
    where not g.deleted and g.id <> '00000000-0000-7d9f-866c-860554389e92'
  on conflict (user_id) do nothing;
