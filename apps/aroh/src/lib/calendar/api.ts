import { invoke } from '@tauri-apps/api/core';

export type ActionStatus = 'planned' | 'done' | 'skipped' | 'missed';

export type SettableStatus = 'planned' | 'done' | 'skipped';

/** One `actions` row as the calendar commands return it, plus its goal title and colour index (0..7). */
export interface ActionView {
	id: string;
	goal_id: string | null;
	title: string;
	notes: string | null;
	start_at: string;
	end_at: string;
	est_minutes: number | null;
	status: ActionStatus;
	seq: number | null;
	done_at: string | null;
	updated_at: string;
	goal_title: string | null;
	goal_color_index: number;
}

/** Reminder settings: master switch, lead minutes (0..=120) and missed grace minutes (0..=240). */
export interface ReminderSettings {
	enabled: boolean;
	lead_min: number;
	missed_grace_min: number;
}

/** Lists the actions overlapping `[from, to)`, both UTC ISO 8601, ordered by start then seq. */
export function listActions(from: string, to: string): Promise<ActionView[]> {
	return invoke<ActionView[]>('calendar_list_actions', { from, to });
}

/** Sets an action to done, skipped or back to planned (the undo). Returns the updated row. */
export function setStatus(id: string, status: SettableStatus): Promise<ActionView> {
	return invoke<ActionView>('calendar_set_status', { id, status });
}

/** Moves an action to new UTC ISO start and end times. Returns the updated row. */
export function moveAction(id: string, startAt: string, endAt: string): Promise<ActionView> {
	return invoke<ActionView>('calendar_move_action', { id, startAt, endAt });
}

/** Reads the reminder settings. */
export function getReminderSettings(): Promise<ReminderSettings> {
	return invoke<ReminderSettings>('calendar_get_reminder_settings');
}

/** Saves the reminder settings and returns what was stored. */
export function setReminderSettings(s: ReminderSettings): Promise<ReminderSettings> {
	return invoke<ReminderSettings>('calendar_set_reminder_settings', { s });
}

/** Asks for notification permission once if the OS has not decided yet. Resolves to whether it is granted. */
export function ensureNotificationPermission(): Promise<boolean> {
	return invoke<boolean>('calendar_ensure_notification_permission');
}
