import { describe, expect, it } from 'vitest';
import { forGoal, openGoalId, tagged } from './goalpage';

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

describe('the goal a page reads for (B96)', () => {
	it('b96: no goal id until the snapshot holds the goal, so a deleted goal sends no engine read', () => {
		const goals = [{ id: 'g1' }];
		expect(openGoalId(null, 'g1')).toBeNull();
		expect(openGoalId({ goals }, 'g1')).toBe('g1');
		expect(openGoalId({ goals }, 'gone')).toBeNull();
		expect(openGoalId({ goals }, undefined)).toBeNull();
	});
});
