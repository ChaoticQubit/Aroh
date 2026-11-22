import { describe, expect, it } from 'vitest';
import type { Snapshot } from './client';
import { formatDay } from '$lib/ui/format';
import { actionViews, checkInData, dueCheckIns, goalHeader, goalView, inboxView } from './views';

const zone = 'UTC';
const goalId = 'g1';

/** A snapshot with one fitness goal, one action, a daily 07:00 weight tracker and one proposal. */
function fixture(): Snapshot {
	return {
		user_id: 'u1',
		categories: [
			{ id: 'c-fit', name: 'Health and Fitness', color: '#4f7a5a', seeded: true },
			{ id: 'c-own', name: 'Music', color: '#000000', seeded: false }
		],
		goals: [{ id: goalId, category_id: 'c-fit', title: 'Six pack by Feb 1', context: null, deadline: '2027-02-01', status: 'active', autonomy: 'balanced', created_at: '2026-11-01T00:00:00Z' }],
		actions: [
			{ id: 'a1', goal_id: goalId, series_id: null, occurrence: null, title: 'Strength session', notes: null, start_at: '2026-11-26T18:00:00Z', end_at: '2026-11-26T19:00:00Z', status: 'planned', locked: false, done_at: null }
		],
		trackers: [{ id: 't1', goal_id: goalId, name: 'Weight', fields: [{ key: 'weight', label: 'Weight', kind: 'number', unit: 'kg', choices: [] }], cadence: { freq: 'daily', weekdays: [], at: '07:00' } }],
		entries: [
			{ id: 'e1', tracker_id: 't1', data: { weight: 82.8 }, source: 'check_in', raw_text: null, logged_at: '2026-11-25T07:00:00Z' },
			{ id: 'e2', tracker_id: 't1', data: { weight: 82.6 }, source: 'check_in', raw_text: null, logged_at: '2026-11-24T07:00:00Z' }
		],
		proposals: [
			{
				id: 'p1',
				goal_id: goalId,
				summary: 'Move the strength session',
				diff: { changes: [{ op: 'move', action_id: 'a1', title: 'Strength session', before: 'Wed 18:00', after: 'Thu 18:00', start_at: null, end_at: null }, { op: 'bogus' }] },
				size: 'small',
				status: 'proposed',
				created_at: '2026-11-26T06:00:00Z'
			}
		],
		preferences: [{ id: 'f1', fact: 'Plan workouts in the evening?', source: 'Planner', noted_at: '2026-11-26T06:00:00Z', confirmed: false }],
		personas: [{ id: 'pe1', goal_id: goalId, name: 'Fitness coach' }],
		providers: [],
		search: [],
		budgets: [],
		settings: {},
		pending_logs: []
	};
}

describe('views', () => {
	it('maps an action row to an ActionView with goal title and category slot', () => {
		const [view] = actionViews(fixture());
		expect(view).toEqual({ id: 'a1', title: 'Strength session', start: '2026-11-26T18:00:00Z', end: '2026-11-26T19:00:00Z', status: 'planned', locked: false, category: 2, goalTitle: 'Six pack by Feb 1' });
	});

	it('asks a due check-in after its time and stops once answered today', () => {
		const s = fixture();
		expect(dueCheckIns(s, { now: '2026-11-26T06:59:00Z', zone })).toEqual([]);
		const [due] = dueCheckIns(s, { now: '2026-11-26T08:15:00Z', zone });
		expect(due.trackerId).toBe('t1');
		expect(due.spec).toMatchObject({ kind: 'question_card', persona: 'Fitness coach', input: { kind: 'number_input', unit: 'kg', min: 61, max: 104 } });
		expect(checkInData(due, 82.4)).toEqual({ weight: 82.4 });
		expect(checkInData(due, null)).toBeNull();
		s.entries.unshift({ id: 'e3', tracker_id: 't1', data: { weight: 82.4 }, source: 'check_in', raw_text: null, logged_at: '2026-11-26T07:30:00Z' });
		expect(dueCheckIns(s, { now: '2026-11-26T08:15:00Z', zone })).toEqual([]);
	});

	it('maps a proposal diff to change views and skips malformed changes', () => {
		const { approvals, suggestions } = inboxView(fixture());
		expect(approvals).toHaveLength(1);
		expect(approvals[0].spec).toMatchObject({ kind: 'proposal_card', changes: [{ op: 'move', title: 'Strength session', before: 'Wed 18:00', after: 'Thu 18:00' }] });
		expect(suggestions[0]).toMatchObject({ id: 'f1', spec: { kind: 'proposal_card', persona: 'Planner', changes: [] } });
		expect(approvals[0].spec).toMatchObject({ persona: 'Fitness coach' });
		expect(inboxView({ ...fixture(), personas: [] }).approvals[0].spec).toMatchObject({ persona: 'Goal planner' });
	});

	it('maps engine insights to insight cards titled by goal', () => {
		const insight = (id: string, source: string, goal: string | null) => ({
			id,
			goal_id: goal,
			run_id: null,
			source,
			title: source === 'edits' ? 'Repeated edits' : 'Missed or late actions',
			body: 'Missed: Core (Sun 10:00).',
			state: 'done',
			created_at: '2026-11-26T06:00:00Z'
		});
		const { insights } = inboxView(fixture(), [], [insight('i1', 'actions', goalId), insight('i2', 'edits', null)]);
		expect(insights).toEqual([
			{ id: 'i1', spec: { kind: 'insight_card', persona: 'Adaptation analyst', title: 'Missed or late actions: Six pack by Feb 1', body: 'Missed: Core (Sun 10:00).' } },
			{ id: 'i2', spec: { kind: 'insight_card', persona: 'Preference analyst', title: 'Repeated edits', body: 'Missed: Core (Sun 10:00).' } }
		]);
	});

	it('hides insights the owner dismissed on this device', () => {
		const s = fixture();
		s.settings.dismissed_insights = 'i1';
		const insight = { id: 'i1', goal_id: null, run_id: null, source: 'review', title: 'Weekly review', body: 'On track.', state: 'done', created_at: '2026-11-26T06:00:00Z' };
		expect(inboxView(s, [], [insight, { ...insight, id: 'i2' }]).insights.map((i) => i.id)).toEqual(['i2']);
	});

	it('derives a goal dashboard with countdown, weight chart and upcoming actions', () => {
		const s = fixture();
		const view = goalView(s, s.goals[0], { now: '2026-11-26T08:15:00Z', zone });
		const kinds = view.sections.flatMap((sec) => sec.items.map((i) => i.kind));
		expect(kinds).toEqual(expect.arrayContaining(['countdown', 'line_chart', 'action_list']));
		const chart = view.sections[0].items.find((i) => i.kind === 'line_chart');
		expect(chart).toMatchObject({ label: 'Weight', unit: 'kg', points: [{ value: 82.6 }, { value: 82.8 }] });
		for (const z of ['America/Los_Angeles', 'Pacific/Auckland', 'Pacific/Kiritimati', 'Pacific/Pago_Pago']) {
			expect(formatDay(goalHeader(s, s.goals[0], z).deadline ?? '', z)).toContain('1 Feb');
		}
	});
});
