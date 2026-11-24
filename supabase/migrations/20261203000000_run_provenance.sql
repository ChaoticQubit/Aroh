alter table public.personas
  add column created_run_id uuid;

alter table public.plugins
  add column created_run_id uuid;

alter table public.runs
  add column plugin_id uuid,
  add column plugin_version bigint check (plugin_version >= 1);
