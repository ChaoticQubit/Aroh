import { describe, expect, it } from 'vitest';
import { forGoal, tagged } from './goalpage';

describe('goal page state keyed by goal id (B87)', () => {
	it('shows a value only on the goal it was read for', () => {
		const plugins = tagged('a', ['plugin of a']);
		expect(forGoal(plugins, 'a', [])).toEqual(['plugin of a']);
		expect(forGoal(plugins, 'b', [])).toEqual([]);
		expect(forGoal(null, 'a', [])).toEqual([]);
		expect(forGoal(plugins, undefined, [])).toEqual([]);
	});

	it('drops a late answer for a goal the page left', () => {
		let held = tagged('b', 'fresh b');
		const late = tagged('a', 'late a');
		held = late.id === 'b' ? late : held;
		expect(forGoal(held, 'b', null)).toBe('fresh b');
		expect(forGoal(late, 'b', null)).toBeNull();
	});
});
