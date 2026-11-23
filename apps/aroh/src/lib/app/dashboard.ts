import { invoke } from '@tauri-apps/api/core';
import type { CategorySlot } from '$lib/ui/contract';
import type { Snapshot } from './client';
import { categoryViews } from './views';

/** How a goal keeps up with its plan and deadline. */
export type Pace = 'ahead' | 'on_track' | 'behind';

/** Done and due action counts of one window. */
export type Counts = { done: number; due: number };

/** A tracker number field, one point per trend week (`null` for a week without a value). */
export type Trend = { tracker_id: string; label: string; unit: string | null; points: (number | null)[]; target: number | null };

/** One goal's figures from `app_dashboard`: counts to date (missed and skipped of the due ones), "YYYY-MM-DD" dates, `minutes` one per week. */
export type GoalStats = {
	goal_id: string;
	done: number;
	due: number;
	missed: number;
	skipped: number;
	open_ended: boolean;
	plan_end: string | null;
	finish: string | null;
	pace: Pace;
	streak: number;
	best_streak: number;
	week: Counts;
	month: Counts;
	minutes: number[];
	trend: Trend | null;
};

/** The dashboard: chart Mondays, oldest first, and every dashboard goal in stable order. */
export type Dashboard = { weeks: string[]; trend_weeks: string[]; goals: GoalStats[] };

/** One goal as the screen draws it: figures, title, category and its chart series (1 to 6, 0 = Other). */
export type DashGoal = GoalStats & { id: string; title: string; deadline: string | null; category: string; categoryId: string; slot: CategorySlot; series: number };

/** A stacked bar series: identity, legend label, fill and minutes per week. */
export type BarSeries = { key: string; label: string; color: string; minutes: number[] };

/** The combined tiles over the shown goals: counts, sums, and the largest streaks. */
export type Totals = { count: number; keeping: number; done: number; due: number; week: Counts; month: Counts; streak: number; best: number };

/** The pace chip words, in chip order. */
export const PACE_LABEL: Record<Pace, string> = { on_track: 'On track', behind: 'Behind', ahead: 'Ahead' };

const SERIES = 6;

/** Reads the dashboard figures over the whole local history. */
export const dashboard = () => invoke<Dashboard>('app_dashboard');

/**
 * Joins `d` with the snapshot's goals and categories. Goals keep the dashboard's stable order
 * and take series 1 to 6 by that order, later ones 0 (Other), so a filter never repaints a goal.
 * Goals missing from the snapshot are dropped.
 */
export function dashGoals(s: Snapshot, d: Dashboard): DashGoal[] {
	const cats = categoryViews(s);
	const byId = new Map(s.goals.map((g) => [g.id, g]));
	return d.goals.flatMap((st) => {
		const g = byId.get(st.goal_id);
		return g ? [{ st, g }] : [];
	}).map(({ st, g }, i) => {
		const cat = (g.category_id && cats.get(g.category_id)) || { name: 'No category', slot: 6 as CategorySlot };
		return { ...st, id: g.id, title: g.title, deadline: g.deadline, category: cat.name, categoryId: g.category_id ?? '', slot: cat.slot, series: i < SERIES ? i + 1 : 0 };
	});
}

/** The filter chips: All, each category present in first-seen order, then the three paces. */
export function chips(goals: DashGoal[]): string[] {
	return ['All', ...new Set(goals.map((g) => g.category)), ...Object.values(PACE_LABEL)];
}

/** The goals a chip shows: all, one category, or one pace. On track includes Ahead, as the On track tile counts. */
export function filterGoals(goals: DashGoal[], chip: string): DashGoal[] {
	if (chip === 'All') return goals;
	const pace = (Object.keys(PACE_LABEL) as Pace[]).find((p) => PACE_LABEL[p] === chip);
	if (pace === 'on_track') return goals.filter((g) => g.pace !== 'behind');
	return goals.filter((g) => (pace ? g.pace === pace : g.category === chip));
}

/** The combined tiles of `goals`: on track or ahead count, summed counts and the largest streaks. */
export function totals(goals: DashGoal[]): Totals {
	const sum = (f: (g: DashGoal) => number) => goals.reduce((s, g) => s + f(g), 0);
	return {
		count: goals.length,
		keeping: goals.filter((g) => g.pace !== 'behind').length,
		done: sum((g) => g.done),
		due: sum((g) => g.due),
		week: { done: sum((g) => g.week.done), due: sum((g) => g.week.due) },
		month: { done: sum((g) => g.month.done), due: sum((g) => g.month.due) },
		streak: Math.max(0, ...goals.map((g) => g.streak)),
		best: Math.max(0, ...goals.map((g) => g.best_streak))
	};
}

/** Done over due as a whole percent, or `null` when nothing was due. */
export function rate(c: Counts): number | null {
	console.assert(c.done <= c.due, 'done above due');
	return c.due ? Math.round((c.done / c.due) * 100) : null;
}

/**
 * The stacked bar series of `goals`: one per goal on `--chart-N` with goals past the sixth folded
 * into one muted Other series, or one per category on `--chart-cat-N`, in first-seen order.
 */
export function barSeries(goals: DashGoal[], by: 'goal' | 'category'): BarSeries[] {
	const add = (a: number[], b: number[]) => a.map((v, i) => v + (b[i] ?? 0));
	if (by === 'category') {
		const cats = new Map<string, BarSeries>();
		for (const g of goals) {
			const c = cats.get(g.categoryId);
			if (c) c.minutes = add(c.minutes, g.minutes);
			else cats.set(g.categoryId, { key: g.categoryId, label: g.category, color: `var(--chart-cat-${g.slot})`, minutes: [...g.minutes] });
		}
		return [...cats.values()];
	}
	const own: BarSeries[] = goals.filter((g) => g.series > 0).map((g) => ({ key: g.id, label: g.title, color: `var(--chart-${g.series})`, minutes: g.minutes }));
	const rest = goals.filter((g) => g.series === 0);
	if (!rest.length) return own;
	return [...own, { key: 'other', label: 'Other', color: 'var(--muted)', minutes: rest.reduce((m, g) => add(m, g.minutes), rest[0].minutes.map(() => 0)) }];
}
