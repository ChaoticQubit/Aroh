import { describe, expect, it } from 'vitest';
import { lockAxis, stepAnchor, swipeStep } from './swipe';

describe('calendar swipe', () => {
	it('moves the anchor a day, a week or a month, across month and year ends', () => {
		expect(stepAnchor('day', '2026-11-30', 1)).toBe('2026-12-01');
		expect(stepAnchor('day', '2027-01-01', -1)).toBe('2026-12-31');
		expect(stepAnchor('week', '2026-12-28', 1)).toBe('2027-01-04');
		expect(stepAnchor('week', '2026-11-03', -1)).toBe('2026-10-27');
		expect(stepAnchor('month', '2026-12-15', 1)).toBe('2027-01-01');
	});

	it('locks to the axis that moved first and ignores small jitter', () => {
		expect(lockAxis(4, 3)).toBeNull();
		expect(lockAxis(14, 4)).toBe('x');
		expect(lockAxis(8, 14)).toBe('y');
		expect(lockAxis(12, 11)).toBe('y');
	});

	it('steps forward on a left swipe and back on a right one, past a quarter width or a flick', () => {
		expect(swipeStep(-120, 390, 0.1)).toBe(1);
		expect(swipeStep(120, 390, 0.1)).toBe(-1);
		expect(swipeStep(-60, 390, 0.1)).toBe(0);
		expect(swipeStep(-40, 390, -0.8)).toBe(1);
		expect(swipeStep(40, 390, -0.8)).toBe(0);
	});
});
