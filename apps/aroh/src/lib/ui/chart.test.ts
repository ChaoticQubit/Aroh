import { describe, expect, it } from 'vitest';
import { niceScale, xOf, yOf } from './chart';

describe('chart scale', () => {
	it('covers values and the target with round ticks', () => {
		const s = niceScale([84, 82.4], 78);
		expect(s.min).toBeLessThanOrEqual(78);
		expect(s.max).toBeGreaterThanOrEqual(84);
		expect(s.ticks).toEqual([78, 80, 82, 84]);
	});
	it('starts bar scales at zero', () => {
		expect(niceScale([118, 162], 150, true).ticks).toEqual([0, 50, 100, 150, 200]);
	});
	it('maps values and indexes to pixels', () => {
		const s = { min: 0, max: 100, ticks: [0, 100] };
		expect(yOf(100, s, 10, 110)).toBe(10);
		expect(yOf(0, s, 10, 110)).toBe(110);
		expect(xOf(0, 3, 0, 100)).toBe(0);
		expect(xOf(2, 3, 0, 100)).toBe(100);
		expect(xOf(0, 1, 0, 100)).toBe(50);
		expect(() => xOf(3, 3, 0, 100)).toThrow(RangeError);
		expect(() => yOf(1, { min: 5, max: 5, ticks: [] }, 0, 1)).toThrow(RangeError);
	});
});
