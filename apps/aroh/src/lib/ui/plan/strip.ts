import type { Action, BusyBlock } from '$lib/app/client';
import { formatDate, formatYmd, minuteOfDay, mondayOf, shiftDay } from '$lib/ui/format';

/** What a block on the plan strip is: a proposed session, busy time, or a fixed (locked) action. */
export type BlockKind = 'plan' | 'busy' | 'fixed';

/** One timed block on one local day, in minutes from midnight; `item` and `fixable` are set on plan sessions only. */
export type StripBlock = { id: string; kind: BlockKind; title: string; date: string; start: number; end: number; source?: string; item?: number; fixable?: boolean };

/** A device all-day event that clears its day (Busy day or Away). */
export type BusyDay = { date: string; title: string };

/** One proposed plan item as the planner writes it: local wall-clock `start`, length, optional RRULE. */
export type PlanItemLike = { title: string; start: string; minutes: number; rrule: string | null };

/** Visible hours of the strip. */
export type Hours = { startHour: number; endHour: number };

/** Pixel scale of the strip: height of an hour and the smallest block height. */
export type Pixels = { hourPx: number; minPx: number };

/** One placed block: pixel top and height inside its day column. */
export type Placed = StripBlock & { y: number; h: number };

/** One day column. */
export type Column = { date: string; past: boolean; busy: BusyDay | null; finish: boolean; blocks: Placed[] };

/** What the summary line says about a plan. */
export type Summary = { minutes: string | null; cadence: string | null; time: string | null; finish: string | null; sessions: number };

const DAY_MIN = 1440;
const MAX_OCCURRENCES = 500;
const WEEKDAYS = ['SU', 'MO', 'TU', 'WE', 'TH', 'FR', 'SA'];
const KNOWN_PARTS = new Set(['FREQ', 'INTERVAL', 'COUNT', 'UNTIL', 'BYDAY', 'WKST']);

/** Weekday of a "YYYY-MM-DD" date, 0 for Sunday. */
function weekday(ymd: string): number {
	return new Date(`${ymd}T12:00:00Z`).getUTCDay();
}

/** Formats minutes from midnight as "HH:MM". */
function hhmm(minute: number): string {
	return `${String(Math.floor(minute / 60)).padStart(2, '0')}:${String(minute % 60).padStart(2, '0')}`;
}

/**
 * The sessions of one plan item in local wall time, clipped to their start day, within
 * `horizonDays` of the item's start. Expands DAILY and WEEKLY rules with INTERVAL, COUNT, UNTIL
 * and BYDAY. Returns null for a rule it cannot expand, so the caller can say so.
 */
export function occurrences(item: PlanItemLike, horizonDays: number): { date: string; start: number; end: number }[] | null {
	const date = item.start.slice(0, 10);
	const start = Number(item.start.slice(11, 13)) * 60 + Number(item.start.slice(14, 16));
	console.assert(/^\d{4}-\d{2}-\d{2}$/.test(date) && start >= 0 && start < DAY_MIN, 'plan item start is local YYYY-MM-DDTHH:MM');
	const at = (d: string) => ({ date: d, start, end: Math.min(DAY_MIN, start + Math.max(1, item.minutes)) });
	const rule = item.rrule?.trim();
	if (!rule) return [at(date)];
	const parts = Object.fromEntries(rule.replace(/^RRULE:/, '').split(';').map((p) => p.split('=') as [string, string]));
	if (!Object.keys(parts).every((k) => KNOWN_PARTS.has(k)) || (parts.FREQ !== 'DAILY' && parts.FREQ !== 'WEEKLY')) return null;
	const interval = Math.max(1, Number(parts.INTERVAL ?? 1));
	const count = parts.COUNT ? Number(parts.COUNT) : Infinity;
	const until = parts.UNTIL ? `${parts.UNTIL.slice(0, 4)}-${parts.UNTIL.slice(4, 6)}-${parts.UNTIL.slice(6, 8)}` : null;
	const days = parts.BYDAY ? parts.BYDAY.split(',').map((d) => WEEKDAYS.indexOf(d.slice(-2))) : [weekday(date)];
	if (days.some((d) => d < 0) || (parts.FREQ === 'DAILY' && parts.BYDAY)) return null;
	const stop = shiftDay(date, horizonDays);
	const out: { date: string; start: number; end: number }[] = [];
	const firstMonday = mondayOf(date);
	for (let d = date; d < stop && out.length < Math.min(count, MAX_OCCURRENCES); d = shiftDay(d, 1)) {
		if (until && d > until) break;
		const offset = Math.round((Date.parse(`${d}T12:00:00Z`) - Date.parse(`${date}T12:00:00Z`)) / 86_400_000);
		const hit =
			parts.FREQ === 'DAILY'
				? offset % interval === 0
				: days.includes(weekday(d)) && Math.round((Date.parse(`${mondayOf(d)}T12:00:00Z`) - Date.parse(`${firstMonday}T12:00:00Z`)) / (7 * 86_400_000)) % interval === 0;
		if (hit) out.push(at(d));
	}
	return out;
}

/**
 * Every session of a plan as strip blocks, numbered by item index; only one-off items are
 * fixable (the Fixed time switch locks the action an item became). `unexpanded` lists the
 * indexes of items whose rule could not be expanded.
 */
export function planBlocks(items: PlanItemLike[], horizonDays: number): { blocks: StripBlock[]; unexpanded: number[] } {
	const blocks: StripBlock[] = [];
	const unexpanded: number[] = [];
	items.forEach((it, i) => {
		const occ = occurrences(it, horizonDays);
		if (!occ) return void unexpanded.push(i);
		occ.forEach((o, k) => blocks.push({ id: `p${i}-${k}`, kind: 'plan', title: it.title, ...o, item: i, fixable: !it.rrule }));
	});
	blocks.sort((a, b) => a.date.localeCompare(b.date) || a.start - b.start);
	return { blocks, unexpanded };
}

/** Splits one UTC interval into per-day pieces in `zone`, in minutes from local midnight. */
function pieces(startIso: string, endIso: string, zone: string): { date: string; start: number; end: number }[] {
	const first = formatYmd(startIso, zone);
	const last = formatYmd(new Date(Date.parse(endIso) - 1).toISOString(), zone);
	const out: { date: string; start: number; end: number }[] = [];
	for (let d = first; d <= last; d = shiftDay(d, 1)) {
		const start = d === first ? minuteOfDay(startIso, zone) : 0;
		const end = d === last ? minuteOfDay(endIso, zone) || DAY_MIN : DAY_MIN;
		if (end > start) out.push({ date: d, start, end });
	}
	return out;
}

/**
 * The busy side of the strip in the plan's `zone`: timed device events as busy blocks (split at
 * midnight), Busy day and Away all-day events as busy days, locked actions of other goals as
 * fixed and their other planned actions as busy. This goal's own actions and finished ones are left out.
 */
export function busyStrip(busy: BusyBlock[], actions: Action[], goalId: string | null, zone: string): { blocks: StripBlock[]; busyDays: BusyDay[] } {
	const blocks: StripBlock[] = [];
	const busyDays: BusyDay[] = [];
	for (const b of busy) {
		if (b.all_day) {
			if (b.day_kind !== 'busy' && b.day_kind !== 'away') continue;
			for (let d = b.start_at.slice(0, 10); d < b.end_at.slice(0, 10); d = shiftDay(d, 1)) busyDays.push({ date: d, title: b.title });
			continue;
		}
		pieces(b.start_at, b.end_at, zone).forEach((p, k) => blocks.push({ id: `b${b.id}-${k}`, kind: 'busy', title: b.title, source: b.group_name ?? undefined, ...p }));
	}
	for (const a of actions) {
		if (a.goal_id === goalId || a.status !== 'planned') continue;
		pieces(a.start_at, a.end_at, zone).forEach((p, k) => blocks.push({ id: `a${a.id}-${k}`, kind: a.locked ? 'fixed' : 'busy', title: a.title, ...p }));
	}
	return { blocks, busyDays };
}

/**
 * The Mondays of every week from the week of `first` to the week of `last` ("YYYY-MM-DD"),
 * one per page of the strip. Returns at least one Monday.
 */
export function weeks(first: string, last: string): string[] {
	const out = [mondayOf(first)];
	const end = mondayOf(last < first ? first : last);
	while (out[out.length - 1] < end) out.push(shiftDay(out[out.length - 1], 7));
	return out;
}

/** Visible hours for `blocks`: 06:00 to 22:00, widened to whole hours so no block is clipped. */
export function scaleFor(blocks: StripBlock[]): Hours {
	let startHour = 6;
	let endHour = 22;
	for (const b of blocks) {
		startHour = Math.min(startHour, Math.floor(b.start / 60));
		endHour = Math.max(endHour, Math.ceil(b.end / 60));
	}
	return { startHour, endHour };
}

/**
 * Places `blocks` of the week starting `monday` into seven day columns on `hours` and `px`, with a
 * minimum height so a short session stays tappable, kept inside the column. Marks days before
 * `today`, the busy days and the `finish` day. Returns seven columns, Monday first.
 */
export function columns(monday: string, blocks: StripBlock[], busyDays: BusyDay[], hours: Hours, px: Pixels, today: string, finish: string | null): Column[] {
	const height = (hours.endHour - hours.startHour) * px.hourPx;
	const top = (minute: number) => Math.min(height, Math.max(0, ((minute - hours.startHour * 60) / 60) * px.hourPx));
	return Array.from({ length: 7 }, (_, i) => {
		const date = shiftDay(monday, i);
		const placed = blocks
			.filter((b) => b.date === date)
			.map((b) => {
				const y = top(b.start);
				const h = Math.max(px.minPx, top(b.end) - y);
				return { ...b, y: Math.max(0, Math.min(y, height - h)), h };
			});
		return { date, past: date < today, busy: busyDays.find((d) => d.date === date) ?? null, finish: date === finish, blocks: placed };
	});
}

/** The most frequent value of a non-empty list; ties go to the earliest. */
function mode(values: number[]): number {
	const counts = new Map<number, number>();
	for (const v of values) counts.set(v, (counts.get(v) ?? 0) + 1);
	return [...counts.entries()].sort((a, b) => b[1] - a[1] || a[0] - b[0])[0][0];
}

/**
 * Reads a plan's sessions for its one-line summary: the minutes (or a range), daily or how many
 * days a week, the usual start time, the finish day (last session) and the session count.
 */
export function summarize(sessions: StripBlock[]): Summary {
	if (!sessions.length) return { minutes: null, cadence: null, time: null, finish: null, sessions: 0 };
	const lengths = sessions.map((s) => s.end - s.start);
	const [lo, hi] = [Math.min(...lengths), Math.max(...lengths)];
	const dates = [...new Set(sessions.map((s) => s.date))].sort();
	const span = Math.round((Date.parse(`${dates[dates.length - 1]}T12:00:00Z`) - Date.parse(`${dates[0]}T12:00:00Z`)) / 86_400_000) + 1;
	const weeksTouched = new Set(dates.map(mondayOf)).size;
	const perWeek = Math.round(dates.length / weeksTouched);
	const cadence = dates.length === span ? 'daily' : `${perWeek} ${perWeek === 1 ? 'day' : 'days'} a week`;
	return { minutes: lo === hi ? `${lo} min` : `${lo}-${hi} min`, cadence, time: hhmm(mode(sessions.map((s) => s.start))), finish: dates[dates.length - 1], sessions: sessions.length };
}

/**
 * The summary as one line, for example "20 min daily · 18:00 · finishes 15 Dec". `today` is
 * "YYYY-MM-DD"; a finish in another year than today carries its year ("finishes 26 Nov 2027").
 */
export function summaryLine(s: Summary, today: string): string {
	if (!s.sessions || !s.finish) return 'No sessions';
	const amount = s.cadence === 'daily' ? `${s.minutes} daily` : `${s.minutes}, ${s.cadence}`;
	const year = s.finish.slice(0, 4) === today.slice(0, 4) ? {} : { year: 'numeric' as const };
	return `${amount} · ${s.time} · finishes ${formatDate(s.finish, { day: 'numeric', month: 'short', ...year })}`;
}
