alter table public.goals
  add column reminders boolean not null default true,
  add column use_calendars boolean not null default true,
  add column run_limit_minutes integer check (run_limit_minutes between 1 and 120);

alter table public.preferences
  add column topic text check (char_length(topic) between 1 and 60),
  add column goal_id uuid,
  add column origin text not null default 'edit' check (origin in ('edit', 'answer')),
  add column blocked boolean not null default false;

alter table public.preferences
  add constraint preferences_blocked_topic check (not blocked or topic is not null);
