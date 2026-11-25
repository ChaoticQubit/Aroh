import { describe, expect, it } from 'vitest';
import type { Snapshot } from './client';
import { formatDay } from '$lib/ui/format';
import { actionViews, busyDayInsights, deadlineAt, profileView, categoryList, checkInData, dueCheckIns, goalHeader, goalProgress, goalView, inboxView } from './views';
import type { GoalStats } from './dashboard';

const zone = 'UTC';
const goalId = 'g1';

/** A snapshot with one fitness goal, one action, a daily 07:00 weight tracker and one proposal. */
function fixture(): Snapshot {
	return {
		user_id: 'u1',
		files: [],
		categories: [
			{ id: 'c-fit', name: 'Health and Fitness', color: '#4f7a5a', seeded: true },
			{ id: 'c-own', name: 'Music', color: '#000000', seeded: false }
		],
		goals: [{ id: goalId, category_id: 'c-fit', title: 'Six pack by Feb 1', context: null, deadline: '2027-02-01', status: 'active', autonomy: 'balanced', created_at: '2026-11-01T00:00:00Z', reminders: true, use_calendars: true, run_limit_minutes: null, ordered: false }],
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
		preferences: [{ id: 'f1', fact: 'Plan workouts in the evening?', source: 'Planner', noted_at: '2026-11-26T06:00:00Z', confirmed: false, topic: null, goal_id: null, origin: 'edit', blocked: false }],
		facts: [
			{ id: 'k1', fact: 'Free minutes a day: 30 min', source: 'answer', noted_at: '2026-11-25T06:00:00Z', confirmed: true, topic: 'daily_minutes', goal_id: goalId, origin: 'answer', blocked: false },
			{ id: 'k2', fact: 'Prefers evening workouts', source: 'edit-analysis', noted_at: '2026-11-20T06:00:00Z', confirmed: true, topic: null, goal_id: null, origin: 'edit', blocked: false },
			{ id: 'k3', fact: 'Body weight', source: 'answer', noted_at: '2026-11-19T06:00:00Z', confirmed: true, topic: 'weight', goal_id: goalId, origin: 'answer', blocked: true }
		],
		personas: [{ id: 'pe1', goal_id: goalId, name: 'Fitness coach' }],
		providers: [],
		search: [],
		budgets: [],
		settings: {},
		pending_logs: []
	};
}

describe('views', () => {
	it('gives a category the slot of its stored hue, seeded or custom', () => {
		const s = fixture();
		s.categories.push({ id: 'c-club', name: 'Reading club', color: '#4A6FA5', seeded: false }, { id: 'c-garden', name: 'Garden', color: '#7a5a4f', seeded: false });
		const slots = Object.fromEntries(categoryList(s).map((c) => [c.name, c.slot]));
		expect(slots).toMatchObject({ 'Health and Fitness': 2, 'Reading club': 1, Garden: 6 });
		expect(slots.Music).toBeGreaterThanOrEqual(1);
	});

	it('maps an action row to an ActionView with goal title and category slot', () => {
		const [view] = actionViews(fixture());
		expect(view).toEqual({ id: 'a1', title: 'Strength session', start: '2026-11-26T18:00:00Z', end: '2026-11-26T19:00:00Z', status: 'planned', locked: false, category: 2, goalTitle: 'Six pack by Feb 1', series: false });
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

	it('lists actions still on a Busy day from today on, one card per Busy block', () => {
		const s = fixture();
		const act = (id: string, title: string, start: string, extra: Partial<Snapshot['actions'][number]> = {}) => ({ ...s.actions[0], id, title, start_at: start, end_at: start.replace(':00:00Z', ':30:00Z'), ...extra });
		s.actions = [
			act('a1', 'Run', '2026-11-28T09:00:00Z'),
			act('a2', 'Dentist', '2026-11-28T15:00:00Z', { locked: true }),
			act('a3', 'Stretch', '2026-11-28T07:00:00Z', { status: 'done' }),
			act('a4', 'Swim', '2026-11-29T09:00:00Z'),
			act('a5', 'Piano', '2026-11-29T20:00:00Z'),
			act('a6', 'Read', '2026-11-29T21:00:00Z'),
			act('a7', 'Walk', '2026-11-24T09:00:00Z'),
			act('a8', 'Yoga', '2026-11-30T09:00:00Z')
		];
		const allDay = (id: string, title: string, start: string, end: string, day_kind: 'busy' | 'away' | null) => ({ id, calendar_id: 'c', event_id: id, title, start_at: start, end_at: end, all_day: true, group_name: null, day_kind });
		s.busy = [
			allDay('0190f2a0-0000-7000-8000-0000000000b1', 'Conference', '2026-11-28T00:00:00Z', '2026-11-30T00:00:00Z', 'busy'),
			allDay('0190f2a0-0000-7000-8000-0000000000b2', 'Old fair', '2026-11-24T00:00:00Z', '2026-11-25T00:00:00Z', 'busy'),
			allDay('0190f2a0-0000-7000-8000-0000000000b3', 'Trip', '2026-11-30T00:00:00Z', '2026-12-01T00:00:00Z', 'away')
		];
		const cards = busyDayInsights(s, zone, '2026-11-26T08:00:00Z');
		expect(cards).toEqual([
			{
				id: '0190f2a0-0000-7000-8000-0000000000b1',
				spec: {
					kind: 'insight_card',
					persona: 'Calendar',
					title: 'No free day for 5 actions',
					body: 'Conference on Sat 28 Nov to Sun 29 Nov: Run, Dentist, Swim and 2 more could not move. Move them by hand or change the day\'s kind.'
				}
			}
		]);
		expect(inboxView(s, [], [], zone, '2026-11-26T08:00:00Z').insights.map((i) => i.id)).toEqual(['0190f2a0-0000-7000-8000-0000000000b1']);
		s.settings.dismissed_insights = '0190f2a0-0000-7000-8000-0000000000b1';
		expect(busyDayInsights(s, zone, '2026-11-26T08:00:00Z')).toEqual([]);
	});

	it('notes a fixed action that clashes with busy time instead of moving it', () => {
		const s = fixture();
		s.actions = [
			{ ...s.actions[0], id: '0190f2a0-0000-7000-8000-0000000000a1', title: 'Dentist', start_at: '2026-11-27T09:00:00Z', end_at: '2026-11-27T10:00:00Z', locked: true },
			{ ...s.actions[0], id: 'a2', title: 'Run', start_at: '2026-11-27T09:30:00Z', end_at: '2026-11-27T10:00:00Z', locked: false },
			{ ...s.actions[0], id: 'a3', title: 'Call', start_at: '2026-11-27T12:00:00Z', end_at: '2026-11-27T13:00:00Z', locked: true }
		];
		s.busy = [{ id: 'b1', calendar_id: 'c', event_id: 'standup', title: 'Standup', start_at: '2026-11-27T09:45:00Z', end_at: '2026-11-27T10:15:00Z', all_day: false, group_name: 'Work', day_kind: null }];
		expect(busyDayInsights(s, zone, '2026-11-26T08:00:00Z')).toEqual([
			{
				id: '0190f2a0-0000-7000-8000-0000000000a1',
				spec: { kind: 'insight_card', persona: 'Calendar', title: 'Fixed action clashes: Dentist', body: 'Dentist on Fri 27 Nov overlaps Standup. Fixed actions are never moved: move it by hand or turn off Fixed time.' }
			}
		]);
	});

	it('hides insights the owner dismissed on this device', () => {
		const s = fixture();
		s.settings.dismissed_insights = 'i1';
		const insight = { id: 'i1', goal_id: null, run_id: null, source: 'review', title: 'Weekly review', body: 'On track.', state: 'done', created_at: '2026-11-26T06:00:00Z' };
		expect(inboxView(s, [], [insight, { ...insight, id: 'i2' }]).insights.map((i) => i.id)).toEqual(['i2']);
	});

	it('says what a waiting run waits for and links to its goal chat, even for a goal not on this device', () => {
		const run = (run_id: string, goal_id: string | null, state: string) => ({ run_id, goal_id, workflow: 'new-goal', state, created_at: '', updated_at: '' });
		const { runs } = inboxView(fixture(), [run('r1', 'g-not-synced', 'waiting_human'), run('r2', null, 'running')], [], 'UTC', '2026-11-26T08:00:00Z', { r1: '3 questions to answer' });
		expect(runs).toEqual([
			{ persona: 'Goal planner', title: 'Planning a new goal (r1)', step: 'Waiting for you: 3 questions to answer', progress: 100, tone: 'warn', working: false, href: '/goals/g-not-synced/chat' },
			{ persona: 'Goal planner', title: 'Planning a new goal (r2)', step: 'Working', progress: 50, tone: 'planned', working: true }
		]);
	});

	it('b97: spins only for runs that still work: failed is red, an owner stop grey, waiting and paused are amber', () => {
		const run = (run_id: string, workflow: string, state: string) => ({ run_id, goal_id: null, workflow, state, created_at: '', updated_at: '' });
		const states = ['failed', 'waiting_human', 'waiting_budget', 'queued', 'running', 'checkpointed'];
		const owner = { ...run('rs', 'ws', 'failed'), stop: 'cancelled' };
		const { runs } = inboxView(fixture(), [...states.map((st, i) => run(`r${i}`, `w${i}`, st)), owner]);
		expect(runs.map((r) => [r.step, r.tone, r.working])).toEqual([
			['Failed', 'missed', false],
			['Waiting for you', 'warn', false],
			['Paused', 'warn', false],
			['Working', 'planned', true],
			['Working', 'planned', true],
			['Working', 'planned', true],
			['Stopped', 'skipped', false]
		]);
	});

	it('shows device notes as insight cards before engine insights and hides dismissed ones', () => {
		const s = fixture();
		const note = (id: string) => ({ id, title: 'Time zone changed', body: 'Now in America/New_York, was Europe/Berlin.', created_at: '2026-11-26T06:00:00Z' });
		s.notes = [note('n2'), note('n1')];
		s.settings.dismissed_insights = 'n1';
		const insight = { id: 'i1', goal_id: null, run_id: null, source: 'review', title: 'Weekly review', body: 'On track.', state: 'done', created_at: '2026-11-26T06:00:00Z' };
		const { insights } = inboxView(s, [], [insight]);
		expect(insights.map((i) => i.id)).toEqual(['n2', 'i1']);
		expect(insights[0].spec).toEqual({ kind: 'insight_card', persona: 'Planner', title: 'Time zone changed', body: 'Now in America/New_York, was Europe/Berlin.' });
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

describe('what Aroh knows about you', () => {
	it('lists kept facts with their source and blocked topics apart, newest first', () => {
		const p = profileView(fixture());
		expect(p.facts).toEqual([
			{ id: 'k1', text: 'Free minutes a day: 30 min', source: 'From your answer on Six pack by Feb 1', date: '2026-11-25T06:00:00Z', blockable: true },
			{ id: 'k2', text: 'Prefers evening workouts', source: 'From your edits', date: '2026-11-20T06:00:00Z', blockable: false }
		]);
		expect(p.blocked).toEqual([{ id: 'k3', label: 'Body weight' }]);
	});
});

describe('goal progress on the Goals list', () => {
	const clock = { now: '2026-11-26T08:00:00Z', zone };
	const stats = (pace: GoalStats['pace']): GoalStats => ({
		goal_id: goalId, done: 9, due: 12, missed: 2, skipped: 1, open_ended: false, plan_end: null, finish: null, pace, streak: 0, best_streak: 0,
		week: { done: 1, due: 2 }, month: { done: 9, due: 12 }, minutes: [], trend: null
	});

	it('says waiting for you while a proposal waits, with the next session today', () => {
		const p = goalProgress(fixture(), [stats('on_track')], clock)[goalId];
		expect(p).toEqual({ tone: 'warn', label: 'Waiting for you', done: 9, due: 12, line: 'Next: Strength session, today 18:00' });
	});

	it('b70: says waiting for you while a run of the goal waits for the owner, with no proposal', () => {
		const s = fixture();
		s.proposals = [];
		const run = (run_id: string, goal_id: string, state: string) => ({ run_id, goal_id, workflow: 'new-goal', state, created_at: '', updated_at: '' });
		expect(goalProgress(s, [stats('on_track')], clock, [run('r1', goalId, 'waiting_human')])[goalId]).toMatchObject({ tone: 'warn', label: 'Waiting for you' });
		expect(goalProgress(s, [stats('on_track')], clock, [run('r1', 'other', 'waiting_human')])[goalId]).toMatchObject({ label: 'On track' });
		expect(goalProgress(s, [stats('on_track')], clock, [])[goalId]).toMatchObject({ label: 'On track' });
	});

	it('b97: a draft goal whose newest run failed says Failed, and one still planning says Working', () => {
		const s = fixture();
		s.proposals = [];
		s.goals[0].status = 'draft';
		const run = (run_id: string, state: string) => ({ run_id, goal_id: goalId, workflow: 'new-goal', state, created_at: '', updated_at: '' });
		expect(goalProgress(s, null, clock, [run('r2', 'failed'), run('r1', 'done')])[goalId]).toMatchObject({ tone: 'missed', label: 'Failed' });
		expect(goalProgress(s, null, clock, [run('r2', 'running')])[goalId]).toMatchObject({ tone: 'planned', label: 'Working' });
	});

	it('takes the pace from the dashboard figures once nothing waits', () => {
		const s = fixture();
		s.proposals = [];
		expect(goalProgress(s, [stats('behind')], clock)[goalId]).toMatchObject({ tone: 'missed', label: 'Behind' });
		expect(goalProgress(s, [stats('ahead')], clock)[goalId]).toMatchObject({ tone: 'done', label: 'Ahead' });
		expect(goalProgress(s, [stats('on_track')], clock)[goalId]).toMatchObject({ tone: 'done', label: 'On track' });
		expect(goalProgress(s, null, clock)[goalId]).toEqual({ tone: 'planned', label: 'Active', done: 0, due: 0, line: 'Next: Strength session, today 18:00' });
	});

	it('labels draft and paused goals and counts missed sessions when nothing is next', () => {
		const s = fixture();
		s.proposals = [];
		s.goals[0].status = 'draft';
		expect(goalProgress(s, null, clock)[goalId]).toMatchObject({ tone: 'planned', label: 'Planning' });
		s.goals[0].status = 'paused';
		expect(goalProgress(s, null, clock)[goalId]).toMatchObject({ tone: 'skipped', label: 'Paused' });
		s.goals[0].status = 'active';
		s.actions[0].status = 'missed';
		s.actions[0].start_at = '2026-11-24T18:00:00Z';
		expect(goalProgress(s, null, clock)[goalId].line).toBe('1 session missed this week');
		s.actions = [];
		expect(goalProgress(s, null, clock)[goalId].line).toBeNull();
	});

	it('shows a later next session with its day', () => {
		const s = fixture();
		s.actions[0].start_at = '2026-11-27T18:00:00Z';
		expect(goalProgress(s, null, clock)[goalId].line).toBe('Next: Strength session, Fri 27 Nov 18:00');
	});
});

describe('deadlineAt (B83)', () => {
	it('puts a deadline at the end of its local day, whatever the zone or a DST change', () => {
		expect(deadlineAt('2026-11-24', 'America/New_York')).toBe('2026-11-25T04:59:59.000Z');
		expect(deadlineAt('2026-11-24', 'Asia/Kolkata')).toBe('2026-11-24T18:29:59.000Z');
		expect(deadlineAt('2026-11-24', 'Pacific/Kiritimati')).toBe('2026-11-24T09:59:59.000Z');
		expect(deadlineAt('2026-03-08', 'America/New_York')).toBe('2026-03-09T03:59:59.000Z');
		expect(deadlineAt('2026-11-01', 'America/New_York')).toBe('2026-11-02T04:59:59.000Z');
		expect(formatDay(deadlineAt('2026-11-24', 'America/New_York'), 'America/New_York')).toBe(formatDay('2026-11-24T17:00:00Z', 'America/New_York'));
	});
});
