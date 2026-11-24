import { sixpackActions } from '$lib/ui/mockdata/sixpack';
import type { ActionView } from '$lib/ui/contract';
import type { BusyView } from '$lib/ui/screens/types';

/** The Monday of the mock week (Q78): Mon 23 to Sun 29 Nov 2026, with today Thu 26. */
export const weekStart = '2026-11-23';

/** The seven days of the mock week, Monday first. */
export const weekDays = Array.from({ length: 7 }, (_, i) => `2026-11-${23 + i}`);

const extra: ActionView[] = [
	{ id: 'w-read-thu', title: 'Read 20 pages', start: '2026-11-26T07:15:00Z', end: '2026-11-26T07:45:00Z', status: 'planned', locked: false, category: 1, goalTitle: 'Read 12 books' },
	{ id: 'w-read-mon', title: 'Read 20 pages', start: '2026-11-23T21:00:00Z', end: '2026-11-23T21:30:00Z', status: 'done', locked: false, category: 1, goalTitle: 'Read 12 books' },
	{ id: 'w-run-sat', title: 'Easy run (catch-up)', start: '2026-11-28T09:00:00Z', end: '2026-11-28T09:40:00Z', status: 'planned', locked: false, category: 2, goalTitle: 'Six pack by Feb 1' }
];

/**
 * The week's actions: the sixpack sample plus an overlap on Thursday morning (Core session and
 * Read 20 pages), a catch-up run on Saturday, and Thursday's Core session not logged yet.
 */
export const weekActions: ActionView[] = [...sixpackActions, ...extra]
	.filter((a) => a.start.slice(0, 10) >= weekDays[0] && a.start.slice(0, 10) <= weekDays[6])
	.map((a) => (a.id === 'a-2026-11-26-0700-2' ? { ...a, status: 'planned' as const } : a))
	.sort((x, y) => Date.parse(x.start) - Date.parse(y.start));

/** Kept device events as busy time. */
export const weekBusy: BusyView[] = [
	{ id: 'b-standup', title: 'Team standup', start: '2026-11-24T09:00:00Z', end: '2026-11-24T09:30:00Z', group: 'Work' },
	{ id: 'b-dentist', title: 'Dentist', start: '2026-11-25T14:00:00Z', end: '2026-11-25T15:00:00Z', group: 'Health' },
	{ id: 'b-dinner', title: 'Dinner with Sam', start: '2026-11-27T19:00:00Z', end: '2026-11-27T21:00:00Z', group: 'Personal' }
];

/** The missed Wednesday Strength session the sheet frame opens. */
export const missedId = 'a-2026-11-25-1800-2';
