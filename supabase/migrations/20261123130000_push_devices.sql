create table public.devices (
  id uuid primary key,
  user_id uuid not null references auth.users on delete cascade,
  platform text not null check (platform = 'ios'),
  token text not null unique check (token ~ '^[0-9a-f]{64,200}$'),
  environment text not null check (environment in ('sandbox', 'production')),
  zone text not null check (char_length(zone) between 1 and 64),
  synced_seq bigint not null check (synced_seq >= 0),
  scheduled_until timestamptz not null,
  created_at timestamptz not null,
  updated_at timestamptz not null
);
create index devices_user_id on public.devices (user_id);
alter table public.devices enable row level security;
revoke all on public.devices from public, anon, authenticated;
grant all on public.devices to service_role;

create table public.push_sent (
  device_id uuid not null references public.devices on delete cascade,
  key text not null check (char_length(key) between 1 and 200),
  sent_at timestamptz not null,
  primary key (device_id, key)
);
create index push_sent_sent_at on public.push_sent (sent_at);
alter table public.push_sent enable row level security;
revoke all on public.push_sent from public, anon, authenticated;
grant all on public.push_sent to service_role;
