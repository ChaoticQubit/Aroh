do $$
begin
  if to_regclass('storage.buckets') is null or to_regclass('storage.objects') is null then
    raise notice 'no storage schema, goal-files limits skipped';
    return;
  end if;
  update storage.buckets set file_size_limit = 10485760 where id = 'goal-files';
  execute $f$create or replace function public.aroh_goal_file_objects(owner_id uuid) returns bigint
    language sql stable security definer set search_path = ''
    as 'select count(*) from storage.objects o where o.bucket_id = ''goal-files'' and (storage.foldername(o.name))[1] = owner_id::text'$f$;
  execute 'revoke all on function public.aroh_goal_file_objects(uuid) from public, anon';
  execute 'grant execute on function public.aroh_goal_file_objects(uuid) to authenticated, service_role';
  execute 'drop policy if exists goal_files_owner_insert on storage.objects';
  execute $p$create policy goal_files_owner_insert on storage.objects for insert to authenticated
    with check (bucket_id = 'goal-files' and (storage.foldername(name))[1] = (select auth.uid())::text
      and public.aroh_goal_file_objects((select auth.uid())) < 20)$p$;
  execute 'drop policy if exists goal_files_enabled on storage.objects';
  execute $p$create policy goal_files_enabled on storage.objects as restrictive for all to authenticated
    using (bucket_id <> 'goal-files' or not (select public.aroh_account_disabled()))
    with check (bucket_id <> 'goal-files' or not (select public.aroh_account_disabled()))$p$;
end;
$$;
