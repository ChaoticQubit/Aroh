/** A value axis: lower and upper bound plus evenly spaced tick values. */
export type Scale = { min: number; max: number; ticks: number[] };

import { formatNumber } from './format';

const STEPS = [1, 2, 2.5, 5, 10];
const LABEL_CHAR_PX = 7.2;

/**
 * Builds a value axis that covers every value and the optional target with round ticks.
 * `fromZero` pins the lower bound at 0, for bar charts. A single value gets 10 % room on
 * each side. Values must be finite and non-empty. Returns a Scale with 3 to 6 ticks.
 */
export function niceScale(values: number[], target?: number, fromZero = false): Scale {
	if (values.length === 0 || !values.every(Number.isFinite)) throw new RangeError('scale needs finite values');
	const all = target === undefined ? values : [...values, target];
	let lo = fromZero ? 0 : Math.min(...all);
	let hi = Math.max(...all);
	if (hi === lo) {
		const pad = Math.max(1, Math.abs(lo) * 0.1);
		hi += pad;
		lo = fromZero ? 0 : lo - pad;
	}
	const raw = (hi - lo) / 4;
	const power = 10 ** Math.floor(Math.log10(raw));
	const step = (STEPS.find((s) => s * power >= raw) ?? 10) * power;
	const min = Math.floor(lo / step) * step;
	const max = Math.ceil(hi / step) * step;
	const ticks: number[] = [];
	for (let t = min; t <= max + step / 2; t += step) ticks.push(Number(t.toFixed(6)));
	return { min, max, ticks };
}

/** Maps a value to a y pixel inside `[top, bottom]`, larger values higher. Returns a pixel offset. */
export function yOf(value: number, scale: Scale, top: number, bottom: number): number {
	const span = scale.max - scale.min;
	if (!(span > 0)) throw new RangeError('empty scale');
	return bottom - ((value - scale.min) / span) * (bottom - top);
}

/** Maps index `i` of `count` evenly across `[left, right]`. A single point sits in the middle. */
export function xOf(i: number, count: number, left: number, right: number): number {
	if (!(i >= 0 && i < count)) throw new RangeError(`index ${i} out of ${count}`);
	return count === 1 ? (left + right) / 2 : left + (i / (count - 1)) * (right - left);
}

/**
 * Left gutter in pixels for the tick labels of `scale`, at least 34: the widest formatted
 * label at the 12 px axis type plus the 12 px gap to the plot. Keeps 5-digit counts unclipped.
 */
export function axisWidth(scale: Scale): number {
	const longest = Math.max(...scale.ticks.map((t) => formatNumber(t).length));
	return Math.max(34, Math.ceil(longest * LABEL_CHAR_PX) + 12);
}
