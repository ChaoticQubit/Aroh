do $$
begin
  if to_regclass('storage.buckets') is null or to_regclass('storage.objects') is null then
    raise notice 'no storage schema, goal-files bucket skipped';
    return;
  end if;
  insert into storage.buckets (id, name, public, file_size_limit)
    values ('goal-files', 'goal-files', false, 26214400)
    on conflict (id) do update set public = false, file_size_limit = 26214400;
  execute 'drop policy if exists goal_files_owner_select on storage.objects';
  execute 'drop policy if exists goal_files_owner_insert on storage.objects';
  execute 'drop policy if exists goal_files_owner_update on storage.objects';
  execute 'drop policy if exists goal_files_owner_delete on storage.objects';
  execute $p$create policy goal_files_owner_select on storage.objects for select to authenticated
    using (bucket_id = 'goal-files' and (storage.foldername(name))[1] = (select auth.uid())::text)$p$;
  execute $p$create policy goal_files_owner_insert on storage.objects for insert to authenticated
    with check (bucket_id = 'goal-files' and (storage.foldername(name))[1] = (select auth.uid())::text)$p$;
  execute $p$create policy goal_files_owner_update on storage.objects for update to authenticated
    using (bucket_id = 'goal-files' and (storage.foldername(name))[1] = (select auth.uid())::text)
    with check (bucket_id = 'goal-files' and (storage.foldername(name))[1] = (select auth.uid())::text)$p$;
  execute $p$create policy goal_files_owner_delete on storage.objects for delete to authenticated
    using (bucket_id = 'goal-files' and (storage.foldername(name))[1] = (select auth.uid())::text)$p$;
end;
$$;
