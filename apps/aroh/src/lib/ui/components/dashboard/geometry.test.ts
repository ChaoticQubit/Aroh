import { describe, expect, it } from 'vitest';
import { barScale, sparkline, stackWeek } from './geometry';

describe('minutes bars', () => {
	it('scales from zero with at least an hour of room', () => {
		expect(barScale([0, 0])).toMatchObject({ min: 0, max: 60 });
		const s = barScale([120, 385]);
		expect(s.min).toBe(0);
		expect(s.max).toBeGreaterThanOrEqual(385);
	});

	it('stacks segments bottom up with a 2px gap and skips empty ones', () => {
		const scale = { min: 0, max: 100, ticks: [0, 50, 100] };
		const series = [
			{ key: 'a', label: 'A', color: 'red', minutes: [50] },
			{ key: 'b', label: 'B', color: 'blue', minutes: [0] },
			{ key: 'c', label: 'C', color: 'green', minutes: [25] }
		];
		const segs = stackWeek(series, 0, scale, 0, 100);
		expect(segs).toEqual([
			{ key: 'a', color: 'red', y: 50, h: 48 },
			{ key: 'c', color: 'green', y: 25, h: 23 }
		]);
		expect(stackWeek(series.map((s) => ({ ...s, minutes: [0] })), 0, scale, 0, 100)).toEqual([]);
	});
});

describe('tracker sparkline', () => {
	it('places points by week and skips weeks without a value', () => {
		const l = sparkline([1, null, 3], undefined, 108, 44);
		expect(l.path).toBe('4,40 104,4');
		expect(l.end).toEqual([104, 4]);
		expect(l.target).toBeNull();
	});

	it('includes the target in the range', () => {
		const l = sparkline([0, 5], 10, 108, 44);
		expect(l.target).toBe(4);
		expect(l.end).toEqual([104, 22]);
	});

	it('centres a single point and draws a flat series mid-height', () => {
		expect(sparkline([null, 7, null], undefined, 108, 44)).toEqual({ path: '54,22', end: [54, 22], target: null });
		expect(sparkline([5, 5], undefined, 108, 44).path).toBe('4,22 104,22');
	});

	it('draws nothing without values', () => {
		expect(sparkline([null, null], 3, 108, 44)).toEqual({ path: '', end: null, target: null });
	});
});
