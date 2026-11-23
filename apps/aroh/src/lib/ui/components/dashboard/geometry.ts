import { niceScale, xOf, yOf, type Scale } from '$lib/ui/chart';
import type { BarSeries } from '$lib/app/dashboard';

/** One stacked segment: its series key and fill, top pixel and height. */
export type Segment = { key: string; color: string; y: number; h: number };

/** A sparkline: SVG polyline points, the last point and the target's y pixel. */
export type Spark = { path: string; end: [number, number] | null; target: number | null };

const GAP = 2;
const PAD = 4;
const MIN_MAX = 60;

/** The minutes axis: from zero, round ticks, at least an hour of room so empty weeks still draw. */
export function barScale(totals: number[]): Scale {
	return niceScale([...totals, MIN_MAX], undefined, true);
}

/**
 * The segments of week `i` stacked bottom up inside `[top, bottom]`, each 2px shorter than its
 * value so a surface gap separates neighbours. Series with no minutes that week are left out.
 */
export function stackWeek(series: BarSeries[], i: number, scale: Scale, top: number, bottom: number): Segment[] {
	let acc = 0;
	return series
		.filter((s) => s.minutes[i] > 0)
		.map((s) => {
			const y0 = yOf(acc, scale, top, bottom);
			acc += s.minutes[i];
			const y = yOf(acc, scale, top, bottom);
			return { key: s.key, color: s.color, y, h: Math.max(0, y0 - y - GAP) };
		});
}

/**
 * The trend line of `points` in a `w` x `h` box with 4px padding: x by week index, y over the
 * range of the values and `target`. Weeks without a value are skipped; a flat series sits
 * mid-height. No values gives an empty path and no target.
 */
export function sparkline(points: (number | null)[], target: number | null | undefined, w: number, h: number): Spark {
	const known = points.flatMap((v, i) => (v === null ? [] : [[i, v] as const]));
	if (!known.length) return { path: '', end: null, target: null };
	const values = known.map(([, v]) => v);
	const lo = Math.min(...values, target ?? Infinity);
	const hi = Math.max(...values, target ?? -Infinity);
	const y = (v: number) => (hi > lo ? h - PAD - ((v - lo) / (hi - lo)) * (h - 2 * PAD) : h / 2);
	const xy = known.map(([i, v]) => [xOf(i, points.length, PAD, w - PAD), y(v)] as [number, number]);
	return { path: xy.map(([a, b]) => `${a},${b}`).join(' '), end: xy[xy.length - 1], target: target == null ? null : y(target) };
}
