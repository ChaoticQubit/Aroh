import { invoke } from '@tauri-apps/api/core';
import type { Card, CardLog } from '$lib/ui/action/card';

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
	reminders: boolean;
	use_calendars: boolean;
	run_limit_minutes: number | null;
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
	card?: Card | null;
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
export type Preference = {
	id: string;
	fact: string;
	source: string;
	noted_at: string;
	confirmed: boolean;
	topic: string | null;
	goal_id: string | null;
	origin: 'edit' | 'answer';
	blocked: boolean;
};

/** A persona row; only its name and goal are shown. */
export type Persona = { id: string; goal_id: string | null; name: string; created_run_id?: string | null };

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

/** A file or link attached to a goal, as the snapshot lists it (never with its text). */
export type GoalFile = {
	id: string;
	goal_id: string;
	name: string;
	kind: 'document' | 'image' | 'audio' | 'video' | 'link';
	storage_path: string | null;
	url: string | null;
	mime: string | null;
	size_bytes: number;
	ingest_status: 'pending' | 'processing' | 'done' | 'failed';
};

/** Every row the screens read, as `app_snapshot` returns it. */
export type Snapshot = {
	user_id: string;
	categories: Category[];
	goals: Goal[];
	files: GoalFile[];
	actions: Action[];
	trackers: Tracker[];
	entries: TrackerEntry[];
	proposals: Proposal[];
	preferences: Preference[];
	facts: Preference[];
	personas: Persona[];
	providers: ProviderSetting[];
	search: SearchSetting[];
	budgets: Budget[];
	settings: Partial<Record<'server_url' | 'goal_budget_usd' | 'calendars' | 'health' | 'supabase_url' | 'supabase_anon_key' | 'dismissed_insights' | 'setup_done' | 'day_suggestions' | 'agent_defaults', string>>;
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

/** The undo of one card save: each changed row before it. */
export type CardUndo = { rows: unknown[] };

/** A stored session log of one action. */
export type ActionLog = CardLog & { id: string; action_id: string; goal_id: string; logged_at: string };

/** What the card page reads: the action, its series' card, the session log and the newest chat edit. */
export type CardPage = { action: Action; series_card: Card | null; log: ActionLog | null; edit: (Proposal & { undo: { rows: unknown[] } | null }) | null };

/** Reads the card page of action `id`. */
export const cardPage = (id: string) => invoke<CardPage>('app_card_page', { id });

/** Saves an in-place card edit for this session or all future ones; returns its undo. */
export const setCard = (id: string, card: Card, scope: 'this' | 'future', minutes: number | null = null) => invoke<CardUndo>('app_set_card', { id, card, minutes, scope });

/** Saves the owner's notes of action `id` as typed. */
export const setNotes = (id: string, notes: string) => invoke<Action>('app_set_notes', { id, notes });

/** Puts back the rows of a card save's undo. */
export const restoreCards = (undo: CardUndo) => invoke<null>('app_restore_cards', { undo });

/** Undoes the coach's applied card chat edit `id`. */
export const undoCardEdit = (id: string) => invoke<Proposal>('app_undo_card_edit', { id });

/** Saves the session log of action `actionId`. */
export const saveLog = (actionId: string, log: CardLog) => invoke<ActionLog>('app_save_log', { actionId, log });

/** Confirms or dismisses a suggested preference. */
export const decidePreference = (id: string, confirm: boolean) => invoke<null>('app_decide_preference', { id, confirm });

/** Edits a kept profile fact's text, dated now. */
export const editFact = (id: string, fact: string) => invoke<Preference>('edit_fact', { id, fact });

/** "Never learn this": turns a kept fact into a blocked topic. */
export const blockTopic = (id: string) => invoke<Preference>('block_topic', { id });

/** "Allow again": removes a blocked topic. */
export const allowTopic = (id: string) => invoke<null>('allow_topic', { id });

/** Sets a goal's autonomy level. */
/** Guesses the owner's seeded category of a new goal from its words, offline. Output: the category id or null. */
export const guessCategory = (title: string, description: string | null) => invoke<string | null>('guess_category', { title, description });

/** Adds an owner category on one of the 6 hues (`#rrggbb`), or the next free hue when `color` is null. */
export const addCategory = (name: string, color: string | null) => invoke<Category>('add_category', { name, color });

export const setAutonomy = (goalId: string, autonomy: Goal['autonomy']) => invoke<Goal>('app_set_autonomy', { goalId, autonomy });

let storage: Promise<boolean> | null = null;

/**
 * Whether Attach can offer photos and files: signed in and the project serves the goal-files
 * bucket. One request per app session, cached; signing in or out asks again.
 */
export function filesAvailable(): Promise<boolean> {
	storage ??= invoke<boolean>('files_available').catch(() => false);
	return storage;
}

/**
 * Attaches `file` to goal `goalId`: the app reads its text off the main thread, uploads it to the
 * owner's bucket and syncs the row. The bytes go raw, the details in a header. Rejects with a reason.
 */
export async function attachFile(goalId: string, file: File): Promise<GoalFile> {
	const bytes = new Uint8Array(await file.arrayBuffer());
	const details = encodeURIComponent(JSON.stringify({ goalId, name: file.name, mime: file.type }));
	return invoke<GoalFile>('attach_file', bytes, { headers: { 'x-aroh-file': details } });
}

/** Attaches the https link `url` to goal `goalId` for the agents to fetch. */
export const attachLink = (goalId: string, url: string) => invoke<GoalFile>('attach_link', { goalId, url });

/** Removes goal file `id`. */
export const removeGoalFile = (id: string) => invoke<void>('remove_goal_file', { id });

/** Deletes goal `goalId` (Q68); `keepActions` keeps its actions on the calendar under Routines. */
export const deleteGoal = (goalId: string, keepActions: boolean) => invoke<void>('delete_goal', { goalId, keepActions });

/** Sets a goal's reminders, calendar reads and run time limit in minutes (null: the Settings limit). */
export const setGoalControls = (goalId: string, reminders: boolean, useCalendars: boolean, runLimitMinutes: number | null) =>
	invoke<Goal>('set_goal_controls', { goalId, reminders, useCalendars, runLimitMinutes });

/** Makes a vendor and model the default provider. Never takes a key. */
export const saveProvider = (vendor: string, model: string) => invoke<ProviderSetting>('app_save_provider', { vendor, model });

/** Sets the monthly budget over all goals in US dollars; null removes it. */
export const saveBudget = (usd: number | null) => invoke<null>('app_save_budget', { usd });

/** Stores one device setting. */
export const setSetting = (name: 'server_url' | 'goal_budget_usd' | 'calendars' | 'health' | 'supabase_url' | 'supabase_anon_key' | 'dismissed_insights' | 'setup_done' | 'day_suggestions' | 'agent_defaults', value: string) => invoke<null>('app_set_setting', { name, value });

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
export const signIn = (email: string, password: string, create: boolean) => ((storage = null), invoke<Account>('app_sign_in', { email, password, create }));

/** Signs in with Apple's native sheet (iPhone only); the device adopts the account and syncs. */
export const signInApple = () => ((storage = null), invoke<Account>('app_sign_in_apple'));

/** Signs out; local rows stay. */
export const signOut = () => ((storage = null), invoke<Account>('app_sign_out'));

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
