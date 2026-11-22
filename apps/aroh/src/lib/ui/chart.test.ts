import { describe, expect, it } from 'vitest';
import { axisWidth, niceScale, xOf, yOf } from './chart';

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
	it('gives a single value round ticks around it', () => {
		const s = niceScale([8432]);
		expect(s.min).toBeLessThan(8432);
		expect(s.max).toBeGreaterThan(8432);
		expect(s.ticks.every(Number.isInteger)).toBe(true);
		expect(s.ticks.length).toBeGreaterThanOrEqual(3);
		expect(niceScale([0]).ticks).toEqual([-1, -0.5, 0, 0.5, 1]);
	});
	it('makes room for the widest tick label', () => {
		expect(axisWidth({ min: 0, max: 100, ticks: [0, 50, 100] })).toBe(34);
		expect(axisWidth({ min: 7500, max: 9500, ticks: [7500, 8000, 9500] })).toBeGreaterThanOrEqual(5 * 7 + 12);
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
