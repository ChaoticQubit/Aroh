create table public.pages (
  id uuid not null,
  user_id uuid not null default auth.uid() references auth.users on delete cascade,
  kind text not null check (kind in ('day', 'action', 'goal', 'free')),
  parent_id uuid,
  created_at timestamptz not null,
  key_version bigint not null check (key_version between 1 and 1000000),
  nonce text not null check (nonce ~ '^[A-Za-z0-9+/=]{16}$'),
  ciphertext text not null check (char_length(ciphertext) between 4 and 16384 and ciphertext ~ '^[A-Za-z0-9+/=]+$'),
  updated_at text collate "C" not null,
  deleted boolean not null default false,
  server_seq bigint not null default 0,
  primary key (user_id, id),
  check (parent_id is null or parent_id <> id)
);
create index pages_parent on public.pages (user_id, parent_id);

create table public.page_blocks (
  id uuid not null,
  user_id uuid not null default auth.uid() references auth.users on delete cascade,
  page_id uuid not null,
  created_at timestamptz not null,
  key_version bigint not null check (key_version between 1 and 1000000),
  nonce text not null check (nonce ~ '^[A-Za-z0-9+/=]{16}$'),
  ciphertext text not null check (char_length(ciphertext) between 4 and 98304 and ciphertext ~ '^[A-Za-z0-9+/=]+$'),
  updated_at text collate "C" not null,
  deleted boolean not null default false,
  server_seq bigint not null default 0,
  primary key (user_id, id)
);
create index page_blocks_page on public.page_blocks (user_id, page_id);

create table public.page_block_history (
  id uuid not null,
  user_id uuid not null default auth.uid() references auth.users on delete cascade,
  block_id uuid not null,
  page_id uuid not null,
  created_at timestamptz not null,
  key_version bigint not null check (key_version between 1 and 1000000),
  nonce text not null check (nonce ~ '^[A-Za-z0-9+/=]{16}$'),
  ciphertext text not null check (char_length(ciphertext) between 4 and 98304 and ciphertext ~ '^[A-Za-z0-9+/=]+$'),
  updated_at text collate "C" not null,
  deleted boolean not null default false,
  server_seq bigint not null default 0,
  primary key (user_id, id)
);
create index page_block_history_block on public.page_block_history (user_id, block_id, created_at);

do $$
declare
  t text;
begin
  foreach t in array array['pages', 'page_blocks', 'page_block_history'] loop
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
