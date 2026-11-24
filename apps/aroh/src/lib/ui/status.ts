import type { ActionView } from './contract';
import type { Tone } from './components/StatusChip.svelte';
import { formatYmd } from './format';

const GRACE_MS = 15 * 60_000;

/**
 * The status chip of one action at `now` in `zone` (Q78): null while a planned action is still
 * ahead or running, "Not logged yet" once it ended until its local day is over (and at least the
 * 15 minute grace after its end), then "Missed"; done, skipped and missed name themselves.
 */
export function logState(a: Pick<ActionView, 'status' | 'start' | 'end'>, now: string, zone: string): { tone: Tone; label: string } | null {
	if (a.status === 'done') return { tone: 'done', label: 'Done' };
	if (a.status === 'skipped') return { tone: 'skipped', label: 'Skipped' };
	if (a.status === 'missed') return { tone: 'missed', label: 'Missed' };
	const at = Date.parse(now);
	const end = Date.parse(a.end);
	console.assert(Number.isFinite(at) && Number.isFinite(end), 'log state needs real times');
	if (at <= end) return null;
	const dayOver = formatYmd(now, zone) > formatYmd(a.start, zone) && at >= end + GRACE_MS;
	return dayOver ? { tone: 'missed', label: 'Missed' } : { tone: 'warn', label: 'Not logged yet' };
}

/** How one action status draws: title classes, a status word with its class, and the category tint in percent. */
export type ActionStyle = { title: string; label: string | null; labelClass: string; tint: number };

const STYLES: Record<ActionView['status'], ActionStyle> = {
	planned: { title: 'text-fg', label: null, labelClass: '', tint: 8 },
	done: { title: 'text-muted line-through', label: 'Done', labelClass: 'text-status-done', tint: 0 },
	skipped: { title: 'text-muted line-through', label: 'Skipped', labelClass: 'text-status-skipped', tint: 0 },
	missed: { title: 'text-status-missed line-through', label: 'Missed', labelClass: 'text-status-missed', tint: 0 }
};

/**
 * The one status-to-style mapping every action view uses (Today, Calendar, goal card), so a
 * status change reads the same everywhere: planned keeps its category tint, done is struck with
 * a green word, skipped grey and missed red, all three untinted. Returns the `ActionStyle`.
 */
export function actionStyle(status: ActionView['status']): ActionStyle {
	const style = STYLES[status];
	console.assert(style !== undefined, `unknown action status ${status}`);
	return style;
}
