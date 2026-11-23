import { describe, expect, it } from 'vitest';
import type { Snapshot } from './client';
import { barSeries, chips, dashGoals, filterGoals, rate, totals, type Dashboard, type GoalStats } from './dashboard';

/** One goal's figures with every field set; `over` replaces some. */
function stats(id: string, over: Partial<GoalStats> = {}): GoalStats {
	return {
		goal_id: id,
		done: 4,
		due: 5,
		missed: 1,
		skipped: 0,
		open_ended: false,
		plan_end: '2026-12-10',
		finish: '2026-12-12',
		pace: 'on_track',
		streak: 2,
		best_streak: 5,
		week: { done: 2, due: 3 },
		month: { done: 4, due: 5 },
		minutes: [0, 30, 60, 0, 0, 90],
		trend: null,
		...over
	};
}

const snapshot = {
	categories: [
		{ id: 'c-learn', name: 'Learning', color: '#4a6fa5', seeded: true },
		{ id: 'c-fit', name: 'Health and Fitness', color: '#4f7a5a', seeded: true }
	],
	goals: ['g1', 'g2', 'g3', 'g4', 'g5', 'g6', 'g7'].map((id, i) => ({
		id,
		category_id: i % 2 ? 'c-fit' : 'c-learn',
		title: `Goal ${id}`,
		context: null,
		deadline: i === 0 ? '2027-01-31' : null,
		status: 'active',
		autonomy: 'ask',
		created_at: '2026-11-01T00:00:00Z',
		reminders: true,
		use_calendars: false,
		run_limit_minutes: null
	}))
} as unknown as Snapshot;

const weeks = ['2026-10-19', '2026-10-26', '2026-11-02', '2026-11-09', '2026-11-16', '2026-11-23'];
const dash: Dashboard = {
	weeks,
	trend_weeks: ['2026-10-12', ...weeks],
	goals: [
		stats('g1', { pace: 'ahead', streak: 6, best_streak: 9 }),
		stats('g2', { pace: 'behind', done: 1, due: 5, week: { done: 0, due: 2 } }),
		stats('g3'),
		stats('g4'),
		stats('g5'),
		stats('g6'),
		stats('g7', { minutes: [10, 10, 10, 10, 10, 10] }),
		stats('ghost')
	]
};

describe('dashboard view model', () => {
	it('joins titles and categories and keeps colour by stable order, folding past six', () => {
		const goals = dashGoals(snapshot, dash);
		expect(goals.map((g) => g.id)).toEqual(['g1', 'g2', 'g3', 'g4', 'g5', 'g6', 'g7']);
		expect(goals[0]).toMatchObject({ title: 'Goal g1', category: 'Learning', categoryId: 'c-learn', slot: 1, series: 1, deadline: '2027-01-31' });
		expect(goals[1]).toMatchObject({ category: 'Health and Fitness', slot: 2, series: 2 });
		expect(goals[6].series).toBe(0);
		const behind = filterGoals(goals, 'Behind');
		expect(behind.map((g) => [g.id, g.series])).toEqual([['g2', 2]]);
	});

	it('offers All, the categories present and the paces', () => {
		const goals = dashGoals(snapshot, dash);
		expect(chips(goals)).toEqual(['All', 'Learning', 'Health and Fitness', 'On track', 'Behind', 'Ahead']);
		expect(filterGoals(goals, 'All')).toHaveLength(7);
		expect(filterGoals(goals, 'Learning').map((g) => g.id)).toEqual(['g1', 'g3', 'g5', 'g7']);
		expect(filterGoals(goals, 'On track').map((g) => g.id)).toEqual(['g1', 'g3', 'g4', 'g5', 'g6', 'g7']);
		expect(filterGoals(goals, 'Ahead').map((g) => g.id)).toEqual(['g1']);
	});

	it('sums the shown goals for the tiles', () => {
		const goals = dashGoals(snapshot, dash).slice(0, 2);
		expect(totals(goals)).toEqual({ count: 2, keeping: 1, done: 5, due: 10, week: { done: 2, due: 5 }, month: { done: 8, due: 10 }, streak: 6, best: 9 });
		expect(totals([])).toEqual({ count: 0, keeping: 0, done: 0, due: 0, week: { done: 0, due: 0 }, month: { done: 0, due: 0 }, streak: 0, best: 0 });
		expect(rate({ done: 2, due: 3 })).toBe(67);
		expect(rate({ done: 0, due: 0 })).toBeNull();
	});

	it('stacks by goal with an Other series, or by category', () => {
		const goals = dashGoals(snapshot, dash);
		const byGoal = barSeries(goals, 'goal');
		expect(byGoal.map((s) => s.key)).toEqual(['g1', 'g2', 'g3', 'g4', 'g5', 'g6', 'other']);
		expect(byGoal[0].color).toBe('var(--chart-1)');
		expect(byGoal[6]).toMatchObject({ label: 'Other', color: 'var(--muted)', minutes: [10, 10, 10, 10, 10, 10] });
		const byCat = barSeries(goals, 'category');
		expect(byCat.map((s) => [s.key, s.label, s.color])).toEqual([
			['c-learn', 'Learning', 'var(--chart-cat-1)'],
			['c-fit', 'Health and Fitness', 'var(--chart-cat-2)']
		]);
		expect(byCat[0].minutes).toEqual([10, 100, 190, 10, 10, 280]);
		expect(barSeries([], 'goal')).toEqual([]);
	});
});
