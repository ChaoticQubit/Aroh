create table public.places (
  id uuid not null,
  user_id uuid not null default auth.uid() references auth.users on delete cascade,
  name text not null check (char_length(name) between 1 and 100),
  address text check (char_length(address) <= 300),
  lat_e6 bigint not null check (lat_e6 between -90000000 and 90000000),
  lon_e6 bigint not null check (lon_e6 between -180000000 and 180000000),
  radius_m bigint not null check (radius_m between 50 and 1000),
  source text not null check (source in ('prompt', 'search')),
  updated_at text collate "C" not null,
  deleted boolean not null default false,
  server_seq bigint not null default 0,
  primary key (user_id, id)
);

create table public.place_links (
  id uuid not null,
  user_id uuid not null default auth.uid() references auth.users on delete cascade,
  place_id uuid not null,
  goal_id uuid not null,
  series_id uuid,
  updated_at text collate "C" not null,
  deleted boolean not null default false,
  server_seq bigint not null default 0,
  primary key (user_id, id)
);

do $$
declare
  t text;
begin
  foreach t in array array['places', 'place_links'] loop
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
