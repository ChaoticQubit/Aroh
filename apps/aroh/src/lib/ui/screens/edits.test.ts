import { describe, expect, it } from 'vitest';
import { dayShift } from './edits';

describe('cross-day drag in the week grid (Q78)', () => {
	it('moves whole days once the finger passes half a column', () => {
		expect(dayShift(20, 44, 3, 7)).toBe(0);
		expect(dayShift(23, 44, 3, 7)).toBe(1);
		expect(dayShift(-70, 44, 3, 7)).toBe(-2);
	});

	it('keeps the action inside the shown days', () => {
		expect(dayShift(400, 44, 5, 7)).toBe(1);
		expect(dayShift(-400, 44, 1, 7)).toBe(-1);
		expect(dayShift(300, 300, 0, 1)).toBe(0);
	});
});
