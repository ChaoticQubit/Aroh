create table public.run_limits (
  user_id uuid primary key references auth.users on delete cascade,
  minutes integer not null check (minutes between 1 and 120),
  updated_at timestamptz not null
);
alter table public.run_limits enable row level security;
revoke all on public.run_limits from public, anon, authenticated;
grant all on public.run_limits to service_role;
