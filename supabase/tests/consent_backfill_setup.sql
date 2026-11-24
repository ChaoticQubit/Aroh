insert into auth.users (id, email) values
  ('00000000-0000-0000-0000-0000000000f1', 'f1@example.com'),
  ('00000000-0000-0000-0000-0000000000f2', 'f2@example.com'),
  ('00000000-0000-0000-0000-0000000000f3', 'f3@example.com');
insert into public.goals (id, user_id, title, status, autonomy, created_at, updated_at) values
  ('0190f2a0-0000-7000-8000-0000000002f1', '00000000-0000-0000-0000-0000000000f1', 'Run', 'active', 'balanced', now(), '1763683200001-0000-ffffffffffffffff'),
  ('00000000-0000-7d9f-866c-860554389e92', '00000000-0000-0000-0000-0000000000f2', 'Routines', 'active', 'ask', now(), '1763683200001-0000-ffffffffffffffff');
insert into public.ai_consents (user_id, granted_at) values ('00000000-0000-0000-0000-0000000000f3', '2026-01-01T00:00:00Z');
insert into public.goals (id, user_id, title, status, autonomy, created_at, updated_at) values
  ('0190f2a0-0000-7000-8000-0000000002f3', '00000000-0000-0000-0000-0000000000f3', 'Swim', 'active', 'balanced', now(), '1763683200001-0000-ffffffffffffffff');
