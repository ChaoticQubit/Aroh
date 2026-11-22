alter table public.busy_blocks
  add column day_kind text check (day_kind in ('reminder', 'busy', 'away'));

alter table public.calendar_rules
  add column day_kind text check (day_kind in ('reminder', 'busy', 'away'));

alter table public.adaptations drop constraint adaptations_source_check;
alter table public.adaptations
  add constraint adaptations_source_check check (source in ('actions', 'rule', 'review', 'edits', 'calendar'));
