/** Hold time before a press on a calendar item turns into a drag, in ms (B122). */
export const HOLD_MS = 300;

/** Finger travel a hold tolerates before it counts as a scroll, in CSS px (B122). */
export const SLOP_PX = 10;

/** Minutes a slot tap and a drag snap to. */
export const SNAP_MIN = 15;

const EDGE_PX = 56;
const MAX_SPEED_PX = 16;

/**
 * Minute of day of the 15 minute slot a tap `y` px below the top of an hour grid lands in, for
 * hours `hourPx` px tall from `startHour` to `endHour`: rounded down, kept inside the grid.
 */
export function slotMinute(y: number, hourPx: number, startHour: number, endHour: number): number {
	console.assert(hourPx > 0 && startHour < endHour, 'an hour grid shows at least one hour');
	const minute = startHour * 60 + Math.floor(((y / hourPx) * 60) / SNAP_MIN) * SNAP_MIN;
	return Math.min(endHour * 60 - SNAP_MIN, Math.max(startHour * 60, minute));
}

/**
 * The instant of wall time `minute` (minutes after midnight) on day `ymd` ("YYYY-MM-DD") in IANA
 * `zone`, as an ISO string in UTC. Daylight saving shifts are found from the zone's own offset.
 */
export function zonedIso(ymd: string, minute: number, zone: string): string {
	const [y, m, d] = ymd.split('-').map(Number);
	console.assert(Number.isInteger(y) && Number.isInteger(m) && Number.isInteger(d), 'a day is YYYY-MM-DD');
	const wall = Date.UTC(y, m - 1, d, 0, minute);
	const offset = (at: number) => {
		const parts = new Intl.DateTimeFormat('en-US', { timeZone: zone, hourCycle: 'h23', year: 'numeric', month: 'numeric', day: 'numeric', hour: 'numeric', minute: 'numeric' }).formatToParts(at);
		const n = (type: string) => Number(parts.find((p) => p.type === type)?.value);
		return Date.UTC(n('year'), n('month') - 1, n('day'), n('hour'), n('minute')) - at;
	};
	const first = wall - offset(wall);
	return new Date(wall - offset(first)).toISOString();
}

/** What a finger that moved `dx`, `dy` px during a hold means: still a hold, or a scroll. */
export function holdMove(dx: number, dy: number): 'wait' | 'scroll' {
	return Math.hypot(dx, dy) > SLOP_PX ? 'scroll' : 'wait';
}

/**
 * What letting go of a press does: `open` for a tap (before the hold ended, or on an item that
 * was draggable at once and travelled at most `SLOP_PX` px), `drop` for a drag that moved by
 * `delta` minutes or `days` days, `none` for an item lifted by the hold and put back.
 */
export function releaseOf(p: { live: boolean; held: boolean; travel: number; delta: number; days: number }): 'open' | 'drop' | 'none' {
	if (!p.live || (!p.held && p.travel <= SLOP_PX)) return 'open';
	return p.delta !== 0 || p.days !== 0 ? 'drop' : 'none';
}

/** Minutes a vertical drag of `dy` px moves an item on hours `hourPx` px tall, in 15 minute steps. */
export function snapMinutes(dy: number, hourPx: number): number {
	console.assert(hourPx > 0, 'hours have a height');
	return Math.round(((dy / hourPx) * 60) / SNAP_MIN) * SNAP_MIN || 0;
}

/**
 * Auto-scroll speed in px per frame for a dragging finger at `y` in a scroller spanning `top` to
 * `bottom`: negative near the top edge, positive near the bottom, 0 in between, faster closer in.
 */
export function edgeSpeed(y: number, top: number, bottom: number): number {
	const up = top + EDGE_PX - y;
	const down = y - (bottom - EDGE_PX);
	if (up > 0) return -Math.round(Math.min(1, up / EDGE_PX) * MAX_SPEED_PX);
	if (down > 0) return Math.round(Math.min(1, down / EDGE_PX) * MAX_SPEED_PX);
	return 0;
}
