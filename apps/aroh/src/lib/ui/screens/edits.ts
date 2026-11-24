import type { ActionView } from '../contract';
import type { ActionEdit, AllDayView, DayKind } from './types';

/**
 * Applies pending local edits to a list of actions: moved or resized actions take their edited
 * start and end, deleted ids drop out. Returns a new list in start order; the input is untouched.
 */
export function applyEdits(actions: ActionView[], edits: Record<string, ActionEdit>, deleted: string[]): ActionView[] {
	return actions
		.filter((a) => !deleted.includes(a.id))
		.map((a) => (edits[a.id] ? { ...a, start: edits[a.id].start, end: edits[a.id].end } : a))
		.sort((x, y) => Date.parse(x.start) - Date.parse(y.start));
}

/** The all-day events that cover a "YYYY-MM-DD" day, with local kind picks applied. */
export function allDayOn(events: AllDayView[], ymd: string, picks: Record<string, DayKind> = {}): AllDayView[] {
	return events.filter((e) => e.start <= ymd && ymd <= e.end).map((e) => (picks[e.id] ? { ...e, kind: picks[e.id] } : e));
}

/** Labels of the three kinds as the chips and the choice card show them. */
export const DAY_KIND_LABEL: Record<DayKind, string> = { reminder: 'Reminder', busy: 'Busy day', away: 'Away' };

/**
 * Whole days a sideways drag of `dx` px moves an action in column `col` of `cols` columns, each
 * `width` px wide: rounded to the nearest column and kept inside the shown days.
 */
export function dayShift(dx: number, width: number, col: number, cols: number): number {
	console.assert(width > 0 && col >= 0 && col < cols, 'a drag starts inside a shown day column');
	const days = Math.round(dx / width);
	return Math.min(cols - 1 - col, Math.max(-col, days)) || 0;
}

/** Moves an ISO instant by whole minutes. Returns an ISO string in UTC. */
export function shiftMinutes(iso: string, minutes: number): string {
	const at = Date.parse(iso);
	if (Number.isNaN(at) || !Number.isInteger(minutes)) throw new RangeError(`bad shift ${iso} ${minutes}`);
	return new Date(at + minutes * 60_000).toISOString();
}
