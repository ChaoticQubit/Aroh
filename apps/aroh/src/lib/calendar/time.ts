/** Minutes in a wall-clock day; grids draw 00:00 to 24:00 whatever the DST offset. */
export const DAY_MINUTES = 1440;

/** IANA name of the device timezone, as the OS reports it to the webview. */
export function deviceTimeZone(): string {
	return Intl.DateTimeFormat().resolvedOptions().timeZone;
}

/** Local midnight of the day that contains `d`. */
export function startOfDay(d: Date): Date {
	return new Date(d.getFullYear(), d.getMonth(), d.getDate());
}

/** The local day `n` days after the day of `d`, at local midnight. DST safe: steps by calendar day. */
export function addDays(d: Date, n: number): Date {
	return new Date(d.getFullYear(), d.getMonth(), d.getDate() + n);
}

/** Monday of the week that contains `d`, at local midnight. */
export function startOfWeek(d: Date): Date {
	const offset = (d.getDay() + 6) % 7;
	return addDays(d, -offset);
}

/** True when `a` and `b` fall on the same local day. */
export function sameDay(a: Date, b: Date): boolean {
	return a.getFullYear() === b.getFullYear() && a.getMonth() === b.getMonth() && a.getDate() === b.getDate();
}

/**
 * UTC ISO bounds `[from, to)` of `days` local days starting at the day of `d`.
 * A DST day is 23 or 25 hours long here, matching what the backend filters on.
 */
export function localRange(d: Date, days = 1): { from: string; to: string } {
	console.assert(days >= 1 && days <= 42, 'range of 1 to 42 days');
	const from = startOfDay(d);
	const to = addDays(from, days);
	console.assert(to.getTime() > from.getTime(), 'range end after start');
	return { from: from.toISOString(), to: to.toISOString() };
}

const timeFmt = new Intl.DateTimeFormat(undefined, { hour: '2-digit', minute: '2-digit', hourCycle: 'h23' });
const dayTitleFmt = new Intl.DateTimeFormat(undefined, { weekday: 'long', day: 'numeric', month: 'long' });
const weekdayFmt = new Intl.DateTimeFormat(undefined, { weekday: 'short' });
const monthFmt = new Intl.DateTimeFormat(undefined, { month: 'long', year: 'numeric' });

/** 24 h local time such as `09:05`, for an ISO string or a Date. Render inside `.num` for tabular digits. */
export function formatTime(t: string | Date): string {
	return timeFmt.format(typeof t === 'string' ? new Date(t) : t);
}

/** Local start and end such as `09:00 to 09:45`. */
export function formatRange(startIso: string, endIso: string): string {
	return `${formatTime(startIso)} to ${formatTime(endIso)}`;
}

/** Long day title such as `Thursday 20 November`. */
export function formatDayTitle(d: Date): string {
	return dayTitleFmt.format(d);
}

/** Short weekday such as `Thu`. */
export function formatWeekday(d: Date): string {
	return weekdayFmt.format(d);
}

/** Month and year such as `November 2026`. */
export function formatMonth(d: Date): string {
	return monthFmt.format(d);
}

/** Wall-clock minutes since local midnight of `day` for instant `t`, clipped to `[0, 1440]`. */
export function minutesInDay(t: Date, day: Date): number {
	const dayStart = startOfDay(day);
	if (t.getTime() <= dayStart.getTime()) return 0;
	if (t.getTime() >= addDays(dayStart, 1).getTime()) return DAY_MINUTES;
	return t.getHours() * 60 + t.getMinutes() + t.getSeconds() / 60;
}

function pad(n: number): string {
	return String(n).padStart(2, '0');
}

/** Local `YYYY-MM-DD` for a date input. */
export function toDateInput(t: string | Date): string {
	const d = typeof t === 'string' ? new Date(t) : t;
	return `${d.getFullYear()}-${pad(d.getMonth() + 1)}-${pad(d.getDate())}`;
}

/** Local `HH:mm` for a time input. */
export function toTimeInput(t: string | Date): string {
	const d = typeof t === 'string' ? new Date(t) : t;
	return `${pad(d.getHours())}:${pad(d.getMinutes())}`;
}

/** UTC ISO instant for a local date input value and time input value, or null when either is malformed. */
export function fromLocalInputs(date: string, time: string): string | null {
	const dm = /^(\d{4})-(\d{2})-(\d{2})$/.exec(date);
	const tm = /^(\d{2}):(\d{2})$/.exec(time);
	if (!dm || !tm) return null;
	const d = new Date(Number(dm[1]), Number(dm[2]) - 1, Number(dm[3]), Number(tm[1]), Number(tm[2]));
	return Number.isNaN(d.getTime()) ? null : d.toISOString();
}

/** An item placed in a day column: wall-clock minutes and its share of the column width. */
export interface Placed<T> {
	item: T;
	startMin: number;
	endMin: number;
	col: number;
	cols: number;
}

/** Smallest drawn block, so a 5 min action stays tappable. */
export const MIN_BLOCK_MINUTES = 20;

/**
 * Lays out the items that touch local `day` in one column.
 * Items that overlap in time share the width: each cluster of overlaps gets as many sub-columns as it needs.
 * Returns placements in start order.
 */
export function layoutDay<T extends { start_at: string; end_at: string }>(items: T[], day: Date): Placed<T>[] {
	const dayStart = startOfDay(day).getTime();
	const dayEnd = addDays(day, 1).getTime();
	const placed: Placed<T>[] = items
		.filter((it) => new Date(it.start_at).getTime() < dayEnd && new Date(it.end_at).getTime() > dayStart)
		.map((item) => {
			const startMin = minutesInDay(new Date(item.start_at), day);
			const endMin = Math.max(minutesInDay(new Date(item.end_at), day), startMin + MIN_BLOCK_MINUTES);
			return { item, startMin, endMin: Math.min(endMin, DAY_MINUTES), col: 0, cols: 1 };
		})
		.sort((a, b) => a.startMin - b.startMin || b.endMin - a.endMin);

	let cluster: Placed<T>[] = [];
	let colEnds: number[] = [];
	let clusterEnd = -1;
	const close = () => {
		for (const p of cluster) p.cols = colEnds.length;
		cluster = [];
		colEnds = [];
	};
	for (const p of placed) {
		if (p.startMin >= clusterEnd) close();
		let col = colEnds.findIndex((end) => end <= p.startMin);
		if (col === -1) {
			col = colEnds.length;
			colEnds.push(p.endMin);
		} else {
			colEnds[col] = p.endMin;
		}
		p.col = col;
		cluster.push(p);
		clusterEnd = Math.max(clusterEnd, p.endMin);
	}
	close();
	console.assert(placed.every((p) => p.col < p.cols && p.endMin > p.startMin), 'layout invariant');
	return placed;
}

/** Hue variable for a goal colour index; indices outside 0..7 wrap. */
export function goalHue(index: number): string {
	const i = ((Math.trunc(index) % 8) + 8) % 8;
	return `var(--goal-${i})`;
}
