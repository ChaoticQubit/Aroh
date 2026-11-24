import { describe, expect, it } from 'vitest';
import type { Snapshot } from './client';
import { decisionFor, locationSettingsView, patternItems, placePrompts, placeRows, routineRows, visitRows, type LocationView } from './location';

const ROUTINES = 'g-routines';

/** A snapshot with one fitness goal, its strength routine and the Routines goal. */
function snap(): Snapshot {
	return {
		user_id: 'u1',
		files: [],
		categories: [
			{ id: 'c-fit', name: 'Health and Fitness', color: '#000', seeded: true },
			{ id: 'c-habit', name: 'Habit', color: '#000', seeded: true }
		],
		goals: [
			{ id: 'g1', category_id: 'c-fit', title: 'Six pack by Feb 1', context: null, deadline: null, status: 'active', autonomy: 'ask', created_at: '2026-11-01T00:00:00Z', reminders: true, use_calendars: true, run_limit_minutes: null },
			{ id: 'g-old', category_id: 'c-fit', title: 'Old goal', context: null, deadline: null, status: 'archived', autonomy: 'ask', created_at: '2026-11-01T00:00:00Z', reminders: true, use_calendars: true, run_limit_minutes: null },
			{ id: ROUTINES, category_id: 'c-habit', title: 'Routines', context: null, deadline: null, status: 'active', autonomy: 'ask', created_at: '1970-01-01T00:00:00Z', reminders: true, use_calendars: true, run_limit_minutes: null }
		],
		actions: [
			{ id: 'a1', goal_id: 'g1', series_id: 's1', occurrence: null, title: 'Strength session', notes: null, start_at: '2026-11-23T17:00:00Z', end_at: '2026-11-23T18:00:00Z', status: 'planned', locked: false, done_at: null },
			{ id: 'a2', goal_id: 'g1', series_id: 's1', occurrence: null, title: 'Strength session', notes: null, start_at: '2026-11-25T17:00:00Z', end_at: '2026-11-25T18:00:00Z', status: 'planned', locked: false, done_at: null }
		],
		trackers: [],
		entries: [],
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

/** A location view with one prompt, one pattern, two places, two routines and three visits. */
function view(): LocationView {
	return {
		routines_goal: ROUTINES,
		settings: { enabled: true, paused: false, nudges: true, watched: 1, visits: 3, ignored: [{ id: 'c9', name: 'Office', address: null }] },
		prompts: [
			{ id: 'p1', guess: 'gym', name: 'Iron Works Gym', address: '1200 Market St', visits: 4, weeks: 2, usual: 'usually around 18:00', lat: 37.77, lon: -122.41 },
			{ id: 'p2', guess: null, name: 'Corner spot', address: null, visits: 3, weeks: 1, usual: 'usually around 07:00', lat: 37.78, lon: -122.42 }
		],
		patterns: [{ id: 'pat1', title: 'Swim laps', place: 'City pool', gap_days: 7, weekday: 'Saturday', local_time: '10:00', duration_minutes: 45, evidence: 5, goal: 'Routines' }],
		places: [
			{ id: 'pl1', name: 'Iron Works Gym', address: '1200 Market St', lat: 37.77, lon: -122.41, watched: true, visits: 4, links: [{ goal_id: 'g1', series_id: 's1' }] },
			{ id: 'pl2', name: 'City pool', address: null, lat: 37.76, lon: -122.44, watched: false, visits: 0, links: [] }
		],
		routines: [
			{ series_id: 'r1', title: 'Swim laps', place: 'City pool', cadence: 'Weekly, Sat', streak: '4 weeks', last_done: '2026-11-21T10:40:00Z', next: { start_at: '2026-11-28T10:00:00Z', status: 'planned' } },
			{ series_id: 'r2', title: 'Morning walk', place: null, cadence: 'Daily', streak: '1 day', last_done: null, next: { start_at: '2026-11-25T07:00:00Z', status: 'missed' } },
			{ series_id: 'r3', title: 'Stretch', place: null, cadence: 'Daily', streak: '0 days', last_done: null, next: { start_at: '2026-11-26T19:00:00Z', status: 'planned' } }
		],
		log: [
			{ id: 'v1', place: 'Iron Works Gym', arrived_at: '2026-11-25T18:00:00Z', left_at: '2026-11-25T19:00:00Z' },
			{ id: 'v2', place: 'Iron Works Gym', arrived_at: '2026-11-26T18:00:00Z', left_at: null }
		]
	};
}

describe('place prompts', () => {
	it('offer each routine, each active goal and Routines as targets, and ask by name without a guess', () => {
		const [gym, corner] = placePrompts(view(), snap());
		expect(gym.guess).toBe('gym');
		expect(gym.question).toBeUndefined();
		expect(gym.address).toBe('1200 Market St');
		expect(gym.targets).toEqual([
			{ value: 'series:g1:s1', label: 'Strength session, Six pack by Feb 1' },
			{ value: 'goal:g1', label: 'Six pack by Feb 1, any action' },
			{ value: `goal:${ROUTINES}`, label: 'Routines' }
		]);
		expect(corner.question).toBe('Is this Corner spot?');
		expect(corner.address).toBe('');
	});

	it('turn a card answer into the Rust decision', () => {
		expect(decisionFor({ kind: 'link', target: 'series:g1:s1' })).toEqual({ kind: 'link', goal_id: 'g1', series_id: 's1' });
		expect(decisionFor({ kind: 'link', target: 'goal:g1' })).toEqual({ kind: 'link', goal_id: 'g1', series_id: null });
		expect(decisionFor({ kind: 'rename', name: 'My gym' })).toEqual({ kind: 'rename', name: 'My gym' });
		expect(decisionFor({ kind: 'ignore' })).toEqual({ kind: 'ignore' });
		expect(decisionFor({ kind: 'link', target: 'nope' })).toBeNull();
		expect(decisionFor('apply')).toBeNull();
	});
});

describe('learned patterns', () => {
	it('become proposal cards with a pattern id, never a proposal row', () => {
		const [card] = patternItems(view());
		expect(card.id).toBe('pattern:pat1');
		expect(card.spec).toEqual({
			kind: 'proposal_card',
			persona: 'Planner',
			summary: 'You swim laps at City pool about every 7 days, usually Saturday ~10:00. Make it a weekly action?',
			changes: [{ op: 'create', title: 'Swim laps at City pool, Routines', before: null, after: 'Every Saturday 10:00, 45 min' }]
		});
	});
});

describe('places', () => {
	it('list links by goal and routine with the category slot, watched state and map positions in the grid', () => {
		const rows = placeRows(view(), snap());
		expect(rows.map((r) => [r.name, r.state, r.address])).toEqual([
			['Iron Works Gym', 'watched', '1200 Market St'],
			['City pool', 'saved', '']
		]);
		expect(rows[0].links).toEqual([{ goal: 'Six pack by Feb 1', slot: 2, action: 'Strength session' }]);
		for (const r of rows) {
			expect(r.x).toBeGreaterThanOrEqual(20);
			expect(r.x).toBeLessThanOrEqual(340);
			expect(r.y).toBeGreaterThanOrEqual(20);
			expect(r.y).toBeLessThanOrEqual(180);
		}
		expect(rows[0].x).toBeGreaterThan(rows[1].x);
		expect(rows[0].y).toBeLessThan(rows[1].y);
	});
});

describe('routines', () => {
	it('keep the streak label with its unit and tone the next occurrence', () => {
		const rows = routineRows(view(), '2026-11-26T12:00:00Z', 'UTC');
		expect(rows.map((r) => [r.title, r.streak, r.tone, r.due])).toEqual([
			['Swim laps', '4 weeks', 'planned', 'Planned'],
			['Morning walk', '1 day', 'missed', 'Missed'],
			['Stretch', '0 days', 'warn', 'Due today']
		]);
		expect(rows[0].lastDone).toBe('2026-11-21T10:40:00Z');
		expect(rows[0].nextDue).toBe('2026-11-28T10:00:00Z');
	});
});

describe('location log and settings', () => {
	it('map visits and switches', () => {
		expect(visitRows(view())).toEqual([
			{ id: 'v1', place: 'Iron Works Gym', arrived: '2026-11-25T18:00:00Z', left: '2026-11-25T19:00:00Z' },
			{ id: 'v2', place: 'Iron Works Gym', arrived: '2026-11-26T18:00:00Z', left: null }
		]);
		const settings = locationSettingsView(view());
		expect(settings).toEqual({ permission: 'always', paused: false, nudges: true, watched: 1, visits: 3, ignored: [{ id: 'c9', name: 'Office', address: '' }] });
		expect(locationSettingsView({ ...view(), settings: { ...view().settings, enabled: false } }).permission).toBe('off');
	});
});
