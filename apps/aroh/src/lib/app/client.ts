import { invoke } from '@tauri-apps/api/core';

/** A goal category row. */
export type Category = { id: string; name: string; color: string; seeded: boolean };

/** A goal row. `deadline` is "YYYY-MM-DD". */
export type Goal = {
	id: string;
	category_id: string | null;
	title: string;
	context: string | null;
	deadline: string | null;
	status: 'draft' | 'active' | 'paused' | 'done' | 'archived';
	autonomy: 'ask' | 'balanced' | 'auto';
	created_at: string;
};

/** An action row; times are RFC 3339 UTC. */
export type Action = {
	id: string;
	goal_id: string;
	series_id: string | null;
	occurrence: string | null;
	title: string;
	notes: string | null;
	start_at: string;
	end_at: string;
	status: 'planned' | 'done' | 'missed' | 'skipped';
	locked: boolean;
	done_at: string | null;
};

/** One field of a tracker's schema. */
export type TrackerField = { key: string; label: string; kind: 'number' | 'text' | 'bool' | 'choice'; unit: string | null; choices: string[] };

/** A tracker row with its check-in cadence; weekdays are "Mon" to "Sun", `at` is local "HH:MM". */
export type Tracker = {
	id: string;
	goal_id: string;
	name: string;
	fields: TrackerField[];
	cadence: { freq: 'daily' | 'weekly'; weekdays: string[]; at: string } | null;
};

/** A tracker entry row: values keyed by field key. */
export type TrackerEntry = { id: string; tracker_id: string; data: Record<string, unknown>; source: 'check_in' | 'log' | 'health'; raw_text: string | null; logged_at: string };

/** A plan change awaiting the owner; `diff` follows the shape in the pod 10 plan. */
export type Proposal = { id: string; goal_id: string; summary: string; diff: unknown; size: 'small' | 'big'; status: string; created_at: string };

/** A suggested user preference. */
export type Preference = { id: string; fact: string; source: string; noted_at: string; confirmed: boolean };

/** A persona row; only its name and goal are shown. */
export type Persona = { id: string; goal_id: string | null; name: string };

/** The chosen model provider. Keys never come back from the store. */
export type ProviderSetting = { id: string; vendor: string; model: string; is_default: boolean };

/** A configured web search provider. */
export type SearchSetting = { id: string; label: string; provider: string; base_url: string | null; priority: number; enabled: boolean };

/** A spend limit row. */
export type Budget = { id: string; scope: 'call' | 'run' | 'goal_month' | 'user_month'; goal_id: string | null; limit_usd_micros: number };

/** A kept device calendar event as busy time; times are RFC 3339 UTC. */
export type BusyBlock = { id: string; calendar_id: string; event_id: string; title: string; start_at: string; end_at: string; all_day: boolean; group_name: string | null; day_kind: 'reminder' | 'busy' | 'away' | null };

/** One device calendar occurrence as the EventKit plugin reads it; times are ISO 8601, `holiday` marks holiday and birthday calendars. */
export type DeviceEvent = { calendar_id: string; calendar: string; event_id: string; title: string; start_at: string; end_at: string; all_day: boolean; rrule: string | null; holiday?: boolean };

/** What one calendar import did; `unmatched` holds one event per series for triage. */
export type CalendarImport = { kept: number; ignored: number; removed: number; unmatched: DeviceEvent[] };

/** Every row the screens read, as `app_snapshot` returns it. */
export type Snapshot = {
	user_id: string;
	categories: Category[];
	goals: Goal[];
	actions: Action[];
	trackers: Tracker[];
	entries: TrackerEntry[];
	proposals: Proposal[];
	preferences: Preference[];
	personas: Persona[];
	providers: ProviderSetting[];
	search: SearchSetting[];
	budgets: Budget[];
	settings: Partial<Record<'server_url' | 'goal_budget_usd' | 'calendars' | 'health' | 'supabase_url' | 'supabase_anon_key' | 'dismissed_insights' | 'setup_done' | 'day_suggestions', string>>;
	pending_logs: { text: string; at: string }[];
	busy?: BusyBlock[];
	notes?: Note[];
	zone?: string | null;
};

/** A device-local Inbox note, such as a time zone change, shown as an insight card. */
export type Note = { id: string; title: string; body: string; created_at: string };

/** What a free-text log became. */
export type Logged = { kind: 'entry'; entry: TrackerEntry } | { kind: 'queued' };

/** Reads the snapshot with actions overlapping `[from, to)`. */
export const snapshot = (from: string, to: string) => invoke<Snapshot>('app_snapshot', { from, to });

/** Runs the start-up tick (seed, materialize, keep off Busy days, mark missed) now. */
export const tick = () => invoke<{ seeded: number; materialized: number; missed: number; moved: number }>('app_tick');

/** Sets an action to planned, done or skipped. */
export const setActionStatus = (id: string, status: 'planned' | 'done' | 'skipped') => invoke<Action>('app_set_action_status', { id, status });

/** Writes a check-in answer: values keyed by tracker field key. */
export const addEntry = (trackerId: string, data: Record<string, unknown>) => invoke<TrackerEntry>('app_add_entry', { trackerId, data });

/** Logs free text as an entry, or queues it for the engine. */
export const logText = (text: string) => invoke<Logged>('app_log_text', { text });

/** Applies or rejects an open proposal. */
export const decideProposal = (id: string, apply: boolean) => invoke<Proposal>('app_decide_proposal', { id, apply });

/** Confirms or dismisses a suggested preference. */
export const decidePreference = (id: string, confirm: boolean) => invoke<null>('app_decide_preference', { id, confirm });

/** Sets a goal's autonomy level. */
export const setAutonomy = (goalId: string, autonomy: Goal['autonomy']) => invoke<Goal>('app_set_autonomy', { goalId, autonomy });

/** Makes a vendor and model the default provider. Never takes a key. */
export const saveProvider = (vendor: string, model: string) => invoke<ProviderSetting>('app_save_provider', { vendor, model });

/** Sets the monthly budget over all goals in US dollars; null removes it. */
export const saveBudget = (usd: number | null) => invoke<null>('app_save_budget', { usd });

/** Stores one device setting. */
export const setSetting = (name: 'server_url' | 'goal_budget_usd' | 'calendars' | 'health' | 'supabase_url' | 'supabase_anon_key' | 'dismissed_insights' | 'setup_done' | 'day_suggestions', value: string) => invoke<null>('app_set_setting', { name, value });

/** Asks for notification permission once and reports whether it is granted. */
export const notificationPermission = () => invoke<boolean>('app_notification_permission');

/** Debug builds only: loads the six-pack sample into an empty store. */
export const devSeed = () => invoke<boolean>('app_dev_seed');

/** Connect Apple Health: permission sheet, `health` on, then an import. Resolves to the status line. */
export const healthConnect = () => invoke<string>('app_health_connect');

/** Debug builds only: adds an Apple Health tracker and writes test samples to HealthKit. */
export const devHealthSeed = () => invoke<null>('app_dev_health_seed');

/** The stored sign-in state, read from the keychain. */
export type Account = { signed_in: boolean; email: string | null; user_id: string | null };

/** The stored sign-in state, without a network call. */
export const account = () => invoke<Account>('app_account');

/** Signs in, or with `create` signs up; the device adopts the account and syncs. */
export const signIn = (email: string, password: string, create: boolean) => invoke<Account>('app_sign_in', { email, password, create });

/** Signs out; local rows stay. */
export const signOut = () => invoke<Account>('app_sign_out');

/** Pushes local changes and pulls the account's rows. */
export const sync = () => invoke<{ pushed: number; pulled: number }>('app_sync');

/** Removes the logs the engine accepted from the local queue. */
export const logsSent = (sent: { text: string; at: string }[]) => invoke<number>('app_logs_sent', { sent });

/** Reads the device calendar occurrences in `[from, to)`, asking for access once. iPhone only. */
export const readCalendar = (from: string, to: string) =>
	invoke<{ events: DeviceEvent[] }>('plugin:aroh-calendar|events', { from, to }).then(
		(r) => r.events,
		(e: unknown) => {
			throw new Error(typeof e === 'object' && e !== null && 'message' in e ? String(e.message) : String(e));
		}
	);

/** Moves or resizes an action to `start`..`end` (RFC 3339); the action is locked so agents leave it. */
export const editAction = (id: string, start: string, end: string) => invoke<Action>('app_edit_action', { id, startAt: start, endAt: end, delete: false });

/** Deletes an action; the server learns from the edit when it syncs. */
export const deleteAction = (id: string) => invoke<Action>('app_edit_action', { id, startAt: null, endAt: null, delete: true });

/** Turns Fixed time on or off for an action; a fixed action is never moved by agents or the app. */
export const setLocked = (id: string, locked: boolean) => invoke<Action>('app_set_locked', { id, locked });

/** What confirming an all-day kind did: actions moved off a Busy day and titles that found no slot. */
export type DaySet = { blocks: number; moved: Action[]; stuck: string[] };

/** Confirms the kind of the all-day event behind busy block `id`. */
export const setDayKind = (id: string, kind: 'reminder' | 'busy' | 'away') => invoke<DaySet>('app_set_day_kind', { id, kind });

/** Matches device occurrences against the synced rules: kept ones become busy blocks. */
export const calendarImport = (events: DeviceEvent[]) => invoke<CalendarImport>('app_calendar_import', { events });
