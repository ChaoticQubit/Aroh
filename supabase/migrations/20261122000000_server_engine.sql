alter table public.jobs
  add column run_id uuid unique references public.runs on delete cascade,
  add column lease_token bigint,
  add column next_run_after timestamptz,
  add column error text;

create sequence public.aroh_lease_seq;
revoke all on sequence public.aroh_lease_seq from public, anon, authenticated;
grant all on sequence public.aroh_lease_seq to service_role;

alter table public.runs
  add column personas jsonb not null default '{}';

alter table public.provider_credentials
  add column vendor text not null check (vendor ~ '^[a-z0-9_-]{1,64}$'),
  add column last4 text not null check (char_length(last4) between 1 and 4);

create table public.user_keys (
  user_id uuid primary key references auth.users on delete cascade,
  wrapped bytea not null,
  nonce bytea not null,
  key_version integer not null check (key_version >= 1),
  created_at timestamptz not null default now()
);
alter table public.user_keys enable row level security;
revoke all on public.user_keys from public, anon, authenticated;
grant all on public.user_keys to service_role;
