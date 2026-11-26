import type { ChangeView } from '$lib/ui/contract';

const OPS: readonly ChangeView['op'][] = ['move', 'resize', 'create', 'delete'];

/** Short local weekday and time of `iso` in `zone`, "Tue 15:00". */
export function when(iso: string, zone: string): string {
	const d = new Date(iso);
	const day = new Intl.DateTimeFormat('en-US', { weekday: 'short', timeZone: zone }).format(d);
	const time = new Intl.DateTimeFormat('en-GB', { hour: '2-digit', minute: '2-digit', hour12: false, timeZone: zone }).format(d);
	return `${day} ${time}`;
}

/** One side of a change as the proposal card shows it: a ready string as given, an engine span `{start_at}` as "Tue 15:00" in `zone`, else null. */
function side(v: unknown, zone: string): string | null {
	if (typeof v === 'string') return v;
	const at = (v as { start_at?: unknown } | null)?.start_at;
	return typeof at === 'string' && !Number.isNaN(Date.parse(at)) ? when(at, zone) : null;
}

/**
 * The change rows of a proposal diff `{ changes }` for the proposal card, the one view the Inbox
 * and the goal chat show (B114). Before and after are strings or engine spans, read in `zone`.
 * Changes with an unknown op or no title are left out.
 */
export function changeViews(diff: unknown, zone: string): ChangeView[] {
	const list = (diff as { changes?: unknown } | null)?.changes;
	if (!Array.isArray(list)) return [];
	return list.flatMap((c) => {
		const op = c?.op;
		if (!OPS.includes(op) || typeof c.title !== 'string') return [];
		return [{ op, title: c.title, before: side(c.before, zone), after: side(c.after, zone) }];
	});
}
