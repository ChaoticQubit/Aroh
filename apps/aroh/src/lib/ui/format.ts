const MS_PER_DAY = 86_400_000;

/** The printed form of a unit: the unit itself, or "" for "count" and no unit. */
export function unitLabel(unit?: string): string {
	return unit === undefined || unit === 'count' ? '' : unit;
}

/** Formats a number with thousands separators and an optional unit, for example "2,400 kcal". The unit "count" is not printed. */
export function formatNumber(value: number, unit?: string): string {
	const text = new Intl.NumberFormat('en-US', { maximumFractionDigits: 1 }).format(value);
	const label = unitLabel(unit);
	return label ? `${text} ${label}` : text;
}

/** Clamps `value` to `min`..`max` and rounds it to the decimals of `step`, so 82.40000001 with step 0.1 becomes 82.4. */
export function snap(value: number, step: number, min = -Infinity, max = Infinity): number {
	if (!Number.isFinite(value) || !(step > 0) || min > max) throw new RangeError(`bad value ${value}, step ${step} or bounds ${min}..${max}`);
	const decimals = (String(step).split('.')[1] ?? '').length;
	return Number(Math.min(max, Math.max(min, value)).toFixed(decimals));
}

/**
 * A typed number as the input keeps it: rounded to the decimals of `step`, never cut to any bound.
 * Input: a finite number and a positive step. Output: the kept number.
 */
export function typedNumber(value: number, step: number): number {
	return snap(value, step);
}

/**
 * The slider range of a number input holding `value`: `min`..`max` widened to include the value,
 * so a typed number always fits the slider. Output: null unless both bounds are given.
 */
export function sliderRange(min: number | undefined, max: number | undefined, value: number | null): { min: number; max: number } | null {
	if (min === undefined || max === undefined) return null;
	if (min > max) throw new RangeError(`bad bounds ${min}..${max}`);
	return value === null ? { min, max } : { min: Math.min(min, value), max: Math.max(max, value) };
}

/**
 * One press of a step button: `delta` added to `value` (or to 0 when empty). Never above any bound;
 * never below `min`, or below `value` when it already sits under `min`. Output: the new number.
 */
export function stepNumber(value: number | null, delta: number, step: number, min: number | undefined): number {
	const from = value ?? 0;
	const floor = min === undefined ? -Infinity : Math.min(min, from);
	return snap(from + delta, step, floor);
}

/** Formats minutes as "45 min", "1 h 30 min" or "2 h". Input must be a non-negative integer. */
export function formatDuration(minutes: number): string {
	if (!Number.isInteger(minutes) || minutes < 0) throw new RangeError(`bad minutes ${minutes}`);
	const h = Math.floor(minutes / 60);
	const m = minutes % 60;
	if (h === 0) return `${m} min`;
	return m === 0 ? `${h} h` : `${h} h ${m} min`;
}

/** Formats an ISO instant as 24-hour "HH:MM" in an IANA zone. */
export function formatTime(iso: string, zone: string): string {
	return new Intl.DateTimeFormat('en-GB', { hour: '2-digit', minute: '2-digit', timeZone: zone }).format(new Date(iso));
}

/** Formats an ISO instant as "Sat 21 Nov" in an IANA zone. */
export function formatDay(iso: string, zone: string): string {
	return new Intl.DateTimeFormat('en-GB', { weekday: 'short', day: 'numeric', month: 'short', timeZone: zone }).format(new Date(iso));
}

/** Formats an ISO instant as its calendar date "YYYY-MM-DD" in an IANA zone. */
export function formatYmd(iso: string, zone: string): string {
	return new Intl.DateTimeFormat('en-CA', { year: 'numeric', month: '2-digit', day: '2-digit', timeZone: zone }).format(new Date(iso));
}

/** Noon UTC of a "YYYY-MM-DD" calendar date, so zone offsets never move it to another day. */
function noon(ymd: string): Date {
	const at = new Date(`${ymd}T12:00:00Z`);
	if (Number.isNaN(at.getTime())) throw new RangeError(`bad date ${ymd}`);
	return at;
}

/** Formats a "YYYY-MM-DD" calendar date with Intl options, for example `{ weekday: 'short' }` gives "Mon". */
export function formatDate(ymd: string, options: Intl.DateTimeFormatOptions): string {
	return new Intl.DateTimeFormat('en-GB', { ...options, timeZone: 'UTC' }).format(noon(ymd));
}

/** Moves a "YYYY-MM-DD" date by whole days, negative for earlier. Returns "YYYY-MM-DD". */
export function shiftDay(ymd: string, days: number): string {
	if (!Number.isInteger(days)) throw new RangeError(`bad days ${days}`);
	return new Date(noon(ymd).getTime() + days * MS_PER_DAY).toISOString().slice(0, 10);
}

/** Moves a "YYYY-MM-DD" date to the first day of the month `months` away. Returns "YYYY-MM-01". */
export function shiftMonth(ymd: string, months: number): string {
	const at = noon(`${ymd.slice(0, 8)}01`);
	at.setUTCMonth(at.getUTCMonth() + months);
	return at.toISOString().slice(0, 10);
}

/** The Monday on or before a "YYYY-MM-DD" date. */
export function mondayOf(ymd: string): string {
	return shiftDay(ymd, -((noon(ymd).getUTCDay() + 6) % 7));
}

/** Whole days from one "YYYY-MM-DD" date to another, negative when `to` is earlier. */
export function daysBetween(from: string, to: string): number {
	return Math.round((noon(to).getTime() - noon(from).getTime()) / MS_PER_DAY);
}

/** Minutes since local midnight of an ISO instant in an IANA zone, 0 to 1439. */
export function minuteOfDay(iso: string, zone: string): number {
	const [h, m] = formatTime(iso, zone).split(':').map(Number);
	return (h % 24) * 60 + m;
}

/** Whole days from `nowIso` to `iso`, rounded down, never negative. */
export function daysUntil(iso: string, nowIso: string): number {
	const diff = Date.parse(iso) - Date.parse(nowIso);
	return Math.max(0, Math.floor(diff / MS_PER_DAY));
}
