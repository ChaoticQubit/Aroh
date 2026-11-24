import { describe, expect, it } from 'vitest';
import type { Action, Snapshot } from '$lib/app/client';
import type { ComponentSpec, ViewSpec } from '$lib/ui/contract';
import { goalView } from '$lib/app/views';
import { parseTree, resolve, SOURCES, type PluginTree } from './resolve';
import allKinds from './fixtures/all-kinds.tree.json';
import fitness from './fixtures/fitness.tree.json';
import music from './fixtures/music.tree.json';
import learning from './fixtures/learning.tree.json';
import finance from './fixtures/finance.tree.json';
import habits from './fixtures/habits.tree.json';

const clock = { now: '2026-11-25T12:00:00Z', zone: 'UTC' };
const goalId = 'g1';
const KINDS = 24;

function action(id: string, day: string, hour: number, status: Action['status']): Action {
	const start = `${day}T${String(hour).padStart(2, '0')}:00:00Z`;
	const end = `${day}T${String(hour + 1).padStart(2, '0')}:00:00Z`;
	return { id, goal_id: goalId, series_id: null, occurrence: null, title: `Session ${id}`, notes: null, start_at: start, end_at: end, status, locked: false, done_at: null };
}

/** A six-pack goal on Wednesday 2026-11-25: weight, protein and water entries, past and next actions. */
function fixture(): Snapshot {
	return {
		user_id: 'u1',
		files: [],
		categories: [{ id: 'c-fit', name: 'Health and Fitness', color: '#4f7a5a', seeded: true }],
		goals: [{ id: goalId, category_id: 'c-fit', title: 'Six pack by Feb 1', context: null, deadline: '2027-02-01', status: 'active', autonomy: 'balanced', created_at: '2026-11-01T00:00:00Z', reminders: true, use_calendars: true, run_limit_minutes: null }],
		actions: [
			action('a1', '2026-11-18', 18, 'done'),
			action('a2', '2026-11-23', 18, 'done'),
			action('a3', '2026-11-24', 7, 'missed'),
			action('a4', '2026-11-25', 18, 'planned'),
			action('a5', '2026-11-26', 7, 'planned'),
			action('a6', '2026-11-27', 18, 'planned'),
			action('a7', '2026-11-29', 10, 'planned'),
			action('a8', '2026-11-30', 18, 'planned')
		],
		trackers: [
			{ id: 't1', goal_id: goalId, name: 'Weight', fields: [{ key: 'weight', label: 'Weight', kind: 'number', unit: 'kg', choices: [] }], cadence: { freq: 'daily', weekdays: [], at: '07:00' } },
			{ id: 't2', goal_id: goalId, name: 'Protein', fields: [{ key: 'protein', label: 'Protein', kind: 'number', unit: 'g', choices: [] }], cadence: null },
			{ id: 't3', goal_id: goalId, name: 'Water', fields: [{ key: 'ml', label: 'Water', kind: 'number', unit: 'ml', choices: [] }, { key: 'note', label: 'Note', kind: 'text', unit: null, choices: [] }], cadence: null }
		],
		entries: [
			{ id: 'e5', tracker_id: 't3', data: { ml: 500 }, source: 'log', raw_text: 'Glass of water', logged_at: '2026-11-25T11:00:00Z' },
			{ id: 'e4', tracker_id: 't3', data: { ml: 250 }, source: 'log', raw_text: null, logged_at: '2026-11-25T09:00:00Z' },
			{ id: 'e3', tracker_id: 't1', data: { weight: 82.4 }, source: 'check_in', raw_text: null, logged_at: '2026-11-25T07:00:00Z' },
			{ id: 'e2', tracker_id: 't2', data: { protein: 140 }, source: 'log', raw_text: null, logged_at: '2026-11-24T20:00:00Z' },
			{ id: 'e1', tracker_id: 't1', data: { weight: 83 }, source: 'check_in', raw_text: null, logged_at: '2026-11-24T07:00:00Z' },
			{ id: 'e0', tracker_id: 't1', data: { weight: 85 }, source: 'check_in', raw_text: null, logged_at: '2026-11-10T07:00:00Z' }
		],
		proposals: [],
		preferences: [],
		facts: [],
		personas: [],
		providers: [],
		search: [],
		budgets: [],
		settings: {},
		pending_logs: []
	};
}

/** The six-pack fixture with its trackers swapped for `tree`'s, each logged yesterday and today. */
function filled(tree: PluginTree): Snapshot {
	const s = fixture();
	s.trackers = tree.trackers.map((t, i) => ({ id: `tt${i}`, goal_id: goalId, name: t.name, fields: t.fields.map((f) => ({ key: f.key, label: f.label, kind: f.kind as 'number', unit: f.unit, choices: [] })), cadence: null }));
	const value = (kind: string, n: number) => (kind === 'number' ? n : kind === 'bool' ? true : 'Bars 1 to 8');
	s.entries = s.trackers.flatMap((t, i) =>
		['2026-11-25T08:00:00Z', '2026-11-24T08:00:00Z'].map((at, j) => ({
			id: `te${i}-${j}`,
			tracker_id: t.id,
			data: Object.fromEntries(t.fields.map((f) => [f.key, value(f.kind, 40 - j * 5)])),
			source: 'log' as const,
			raw_text: null,
			logged_at: at
		}))
	);
	return s;
}

function items(view: ViewSpec): ComponentSpec[] {
	return view.sections.flatMap((s) => s.items);
}

function hasBinding(v: unknown): boolean {
	if (Array.isArray(v)) return v.some(hasBinding);
	if (v && typeof v === 'object') return 'bind' in v || Object.values(v).some(hasBinding);
	return false;
}

describe('plugin resolver', () => {
	it('resolves every kind and every source with nothing left bound', () => {
		const s = fixture();
		const view = resolve(parseTree(allKinds), s, s.goals[0], clock);
		const kinds = new Set(items(view).map((i) => i.kind));
		expect(kinds.size).toBe(KINDS);
		expect(hasBinding(view)).toBe(false);
		expect(view.title).toBe('Six pack by Feb 1');
		const byLabel = (label: string) => items(view).find((i) => 'label' in i && i.label === label);
		expect(byLabel('Water')).toMatchObject({ kind: 'counter', value: 750, target: 2000 });
		expect(byLabel('Weight')).toMatchObject({ kind: 'number_input', value: 82.4 });
		const stat = items(view).find((i) => i.kind === 'stat_tile');
		expect(stat, 'change covers the last 7 days, as the tile says').toMatchObject({ value: 82.4, delta: expect.closeTo(-0.6, 5) });
		const target = items(view).find((i) => i.kind === 'target_vs_actual');
		expect(target).toMatchObject({ target: 78, actual: 82.4 });
		const strip = items(view).find((i) => i.kind === 'week_strip');
		expect(strip).toMatchObject({ startDay: '2026-11-23' });
		expect(strip && strip.kind === 'week_strip' && strip.actions.map((a) => a.id)).toEqual(['a2', 'a3', 'a4', 'a5', 'a6', 'a7']);
		const today = byLabel('Today');
		expect(today && today.kind === 'action_list' && today.actions.map((a) => a.id)).toEqual(['a4']);
		const next = byLabel('Next');
		expect(next && next.kind === 'action_list' && next.actions.map((a) => a.id)).toEqual(['a4', 'a5']);
		const water = byLabel('Water logged');
		expect(water && water.kind === 'streak_heatmap' && water.days.filter((d) => d.level > 0).map((d) => d.date)).toEqual(['2026-11-25']);
		const log = byLabel('Weigh-ins');
		expect(log && log.kind === 'log_list' && log.entries.map((e) => e.value)).toEqual([82.4, 83, 85]);
		expect(items(view).find((i) => i.kind === 'question_card')).toMatchObject({ input: { kind: 'number_input', value: null } });
	});

	it('lists the sources the Rust crate checks', () => {
		expect(SOURCES).toHaveLength(12);
	});

	it('throws on an unknown source', () => {
		const tree = parseTree(allKinds);
		const broken = { ...tree, sections: [{ title: 'X', items: [{ kind: 'stat_tile', label: 'X', unit: 'kg', value: { bind: 'median', tracker: 'Weight', field: 'weight' } }] }] };
		const s = fixture();
		expect(() => resolve(broken, s, s.goals[0], clock)).toThrow('unknown binding source `median`');
	});

	it('drops items without data and sections without items', () => {
		const s = { ...fixture(), entries: [], actions: [], goals: [{ ...fixture().goals[0], deadline: null }] };
		const view = resolve(parseTree(fitness), s, s.goals[0], clock);
		expect(view.sections.map((x) => x.title)).toEqual(['Upcoming']);
		expect(items(view)).toEqual([{ kind: 'action_list', label: 'Next sessions', actions: [] }]);
	});

	it('renders the fitness template like the derived dashboard', () => {
		const s = fixture();
		s.trackers = s.trackers.filter((t) => t.name !== 'Water');
		s.entries = s.entries.filter((e) => e.tracker_id !== 't3');
		const fromPlugin = resolve(parseTree(fitness), s, s.goals[0], clock);
		const derived = goalView(s, s.goals[0], clock);
		const withoutCountdownLabel = (v: ViewSpec) => ({
			...v,
			sections: v.sections.map((sec) => ({ ...sec, items: sec.items.map((i) => (i.kind === 'countdown' ? { ...i, label: '' } : i)) }))
		});
		expect(items(derived).map((i) => i.kind)).toEqual(['countdown', 'progress_ring', 'line_chart', 'line_chart', 'streak_heatmap', 'action_list']);
		expect(withoutCountdownLabel(fromPlugin)).toEqual(withoutCountdownLabel(derived));
	});

	it.each([
		['music', music],
		['learning', learning],
		['finance', finance],
		['habits', habits]
	])('resolves every item of the %s template when its trackers have data', (_name, json) => {
		const tree = parseTree(json);
		const s = filled(tree);
		const view = resolve(tree, s, s.goals[0], clock);
		const declared = tree.sections.flatMap((sec) => sec.items.map((i) => (i as { kind: string }).kind));
		expect(view.sections.map((sec) => sec.title)).toEqual(tree.sections.map((sec) => sec.title));
		expect(items(view).map((i) => i.kind)).toEqual(declared);
		expect(hasBinding(view)).toBe(false);
	});

	it('rejects a tree of another format', () => {
		expect(() => parseTree({ ...fitness, format: 2 })).toThrow('plugin tree format');
		expect(() => parseTree(null)).toThrow('plugin tree format');
	});
});
