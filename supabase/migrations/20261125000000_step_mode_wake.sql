create or replace function public.aroh_jobs_wake()
returns trigger
language plpgsql
security definer
set search_path = ''
as $$
declare
  engine_url text;
  service_jwt text;
begin
  if new.status <> 'queued' or (tg_op = 'UPDATE' and old.status in ('queued', 'running')) then
    return null;
  end if;
  if to_regnamespace('net') is null or to_regclass('vault.decrypted_secrets') is null then
    return null;
  end if;
  execute 'select decrypted_secret from vault.decrypted_secrets where name = $1' into engine_url using 'aroh_engine_url';
  execute 'select decrypted_secret from vault.decrypted_secrets where name = $1' into service_jwt using 'aroh_service_jwt';
  if engine_url is null or service_jwt is null then
    return null;
  end if;
  execute 'select net.http_post(url := $1, headers := $2, body := $3, timeout_milliseconds := 290000)'
    using engine_url || '/v1/hooks/enqueue',
      jsonb_build_object('Content-Type', 'application/json', 'Authorization', 'Bearer ' || service_jwt),
      jsonb_build_object('type', tg_op, 'table', 'jobs');
  return null;
exception when others then
  raise warning 'aroh jobs wake skipped: %', sqlerrm;
  return null;
end;
$$;
revoke all on function public.aroh_jobs_wake() from public, anon, authenticated;
create trigger jobs_wake after insert or update of status on public.jobs for each row execute function public.aroh_jobs_wake();

do $$
begin
  if exists (select 1 from pg_available_extensions where name = 'pg_cron')
    and exists (select 1 from pg_available_extensions where name = 'pg_net')
    and exists (select 1 from pg_namespace where nspname = 'vault') then
    create extension if not exists pg_cron;
    create extension if not exists pg_net;
    perform cron.schedule('aroh-push', '* * * * *', $job$
      select net.http_post(
        url := (select decrypted_secret from vault.decrypted_secrets where name = 'aroh_engine_url') || '/v1/push/tick',
        headers := jsonb_build_object(
          'Content-Type', 'application/json',
          'Authorization', 'Bearer ' || (select decrypted_secret from vault.decrypted_secrets where name = 'aroh_service_jwt')
        ),
        body := '{}'::jsonb,
        timeout_milliseconds := 60000
      )
    $job$);
  end if;
exception when others then
  raise notice 'aroh-push not scheduled: %', sqlerrm;
end;
$$;
