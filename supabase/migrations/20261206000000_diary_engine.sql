alter table public.diary_keys add column agents_read boolean not null default true;
alter table public.diary_keys alter column agents_read set default false;
