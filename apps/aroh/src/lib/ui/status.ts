import type { ActionView } from './contract';

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
