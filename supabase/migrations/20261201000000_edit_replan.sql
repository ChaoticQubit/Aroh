alter table public.action_events
  add column replan_checked_at timestamptz,
  add column replan_run_id uuid references public.runs on delete set null;

create index action_events_replan_pending on public.action_events (user_id, id)
  where replan_checked_at is null and kind in ('move', 'resize', 'delete');

create index action_events_action on public.action_events (user_id, action_id, id desc);
