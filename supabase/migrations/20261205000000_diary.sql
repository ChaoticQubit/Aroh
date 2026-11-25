alter table public.proposals
  add column sealed_summary jsonb check (sealed_summary is null or jsonb_typeof(sealed_summary) = 'object');

alter table public.runs
  add column sealed boolean not null default false;

create table public.diary_entries (
  id uuid not null,
  user_id uuid not null default auth.uid() references auth.users on delete cascade,
  day text not null check (day ~ '^[0-9]{4}-[0-9]{2}-[0-9]{2}$'),
  at timestamptz not null,
  key_version bigint not null check (key_version between 1 and 1000000),
  nonce text not null check (nonce ~ '^[A-Za-z0-9+/=]{16}$'),
  ciphertext text not null check (char_length(ciphertext) between 4 and 16384 and ciphertext ~ '^[A-Za-z0-9+/=]+$'),
  updated_at text collate "C" not null,
  deleted boolean not null default false,
  server_seq bigint not null default 0,
  primary key (user_id, id)
);
create index diary_entries_day on public.diary_entries (user_id, day);

create table public.diary_days (
  id uuid not null,
  user_id uuid not null default auth.uid() references auth.users on delete cascade,
  day text not null check (day ~ '^[0-9]{4}-[0-9]{2}-[0-9]{2}$'),
  key_version bigint not null check (key_version between 1 and 1000000),
  nonce text not null check (nonce ~ '^[A-Za-z0-9+/=]{16}$'),
  ciphertext text not null check (char_length(ciphertext) between 4 and 98304 and ciphertext ~ '^[A-Za-z0-9+/=]+$'),
  updated_at text collate "C" not null,
  deleted boolean not null default false,
  server_seq bigint not null default 0,
  primary key (user_id, id)
);
create index diary_days_day on public.diary_days (user_id, day);

do $$
declare
  t text;
begin
  foreach t in array array['diary_entries', 'diary_days'] loop
    execute format('alter table public.%I add constraint %I check (updated_at ~ ''^[0-9]{13}-[0-9a-f]{4}-[0-9a-f]{16}$'')', t, t || '_hlc');
    execute format('create trigger %I before insert or update on public.%I for each row execute function public.aroh_lww()', t || '_lww', t);
    execute format('create index %I on public.%I (user_id, server_seq)', t || '_server_seq', t);
    execute format('alter table public.%I enable row level security', t);
    execute format('revoke all on public.%I from public, anon, authenticated', t);
    execute format('grant all on public.%I to service_role', t);
    execute format('create policy %I on public.%I for select to authenticated using (user_id = (select auth.uid()))', t || '_select', t);
    execute format('create policy %I on public.%I for insert to authenticated with check (user_id = (select auth.uid()))', t || '_insert', t);
    execute format('create policy %I on public.%I for update to authenticated using (user_id = (select auth.uid())) with check (user_id = (select auth.uid()))', t || '_update', t);
    execute format('create policy %I on public.%I for delete to authenticated using (user_id = (select auth.uid()))', t || '_delete', t);
    execute format('grant select, insert, update, delete on public.%I to authenticated', t);
  end loop;
end;
$$;

create table public.diary_keys (
  user_id uuid primary key references auth.users on delete cascade,
  key_version bigint not null check (key_version between 1 and 1000000),
  sealed bytea not null,
  nonce bytea not null check (octet_length(nonce) = 12),
  created_at timestamptz not null default now()
);
alter table public.diary_keys enable row level security;
revoke all on public.diary_keys from public, anon, authenticated;
grant all on public.diary_keys to service_role;
