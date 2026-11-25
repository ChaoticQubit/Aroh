import { describe, expect, it } from 'vitest';
import { activeMs, answersReason, chipFor, goalChip, goalStatus, markRead, mergeRuns, originRun, pickRun, planWaits, runChip, runState, settledMark, type Chip } from './goalcard';
import type { RunEvent, RunSummary, RunView } from './engine';

const at = (seq: number, ms: number, type: string, step: string): RunEvent => ({ seq, at_ms: ms, body: { type, step } });
const summary = (run_id: string, state: string): RunSummary => ({ run_id, goal_id: 'g', workflow: 'new-goal', state, created_at: '', updated_at: '' });
const view = (patch: Partial<RunView>): RunView => ({ run_id: 'r', goal_id: 'g', workflow: 'new-goal', status: { status: 'running' }, waiting: [], events: [], usage: { cost_usd_micros: 10_000 }, ...patch });
const IDLE: Chip = { tone: 'planned', label: 'Active' };
const shared = (v: RunView | null) => chipFor({ status: 'active' }, v ? runState(v.status.status, v.status.reason?.kind) : null, false);

describe('one goal chip on every screen (b97)', () => {
	const GOALS = ['draft', 'active', 'paused', 'done', 'archived'] as const;
	const STATES = [null, 'queued', 'running', 'checkpointed', 'waiting_human', 'waiting_budget', 'failed', 'stopped', 'done'];
	const RUN: Record<string, [string, Chip['tone']] | null> = {
		queued: ['Working', 'planned'],
		running: ['Working', 'planned'],
		checkpointed: ['Working', 'planned'],
		waiting_human: ['Waiting for you', 'warn'],
		waiting_budget: ['Paused', 'warn'],
		failed: ['Failed', 'missed'],
		stopped: ['Stopped', 'skipped'],
		done: null
	};
	const GOAL: Record<string, [string, Chip['tone']]> = { draft: ['Planning', 'planned'], active: ['Active', 'planned'], paused: ['Paused', 'skipped'], done: ['Done', 'done'], archived: ['Archived', 'skipped'] };
	const table = GOALS.flatMap((g) => STATES.map((st) => [g, st] as const));

	it.each(table)('goal %s with run %s', (g, st) => {
		const settled = g === 'paused' || g === 'done' || g === 'archived';
		const [label, tone] = settled ? GOAL[g] : ((st && RUN[st]) ?? GOAL[g]);
		expect(chipFor({ status: g }, st, false)).toEqual({ label, tone });
		const runs = st === 'stopped' ? [{ ...summary('r1', 'failed'), stop: 'cancelled' }] : st ? [summary('r1', st)] : [];
		expect(goalChip({ status: g }, runs, false)).toEqual({ label, tone });
	});

	it('a failed newest run says Failed, never Planning or Working, for a draft goal', () => {
		expect(goalChip({ status: 'draft' }, [summary('new', 'failed'), summary('old', 'done')], false)).toEqual({ tone: 'missed', label: 'Failed' });
	});

	it('follows the run pickRun picks: an older waiting run wins over a newer failed one', () => {
		expect(goalChip({ status: 'active' }, [summary('new', 'failed'), summary('old', 'waiting_human')], false)).toEqual({ tone: 'warn', label: 'Waiting for you' });
	});

	it('a proposed proposal waits after Working and before Paused, Failed and the goal label', () => {
		expect(chipFor({ status: 'active' }, 'running', true).label).toBe('Working');
		expect(chipFor({ status: 'active' }, 'waiting_budget', true).label).toBe('Waiting for you');
		expect(chipFor({ status: 'active' }, 'failed', true)).toEqual({ tone: 'warn', label: 'Waiting for you' });
		expect(chipFor({ status: 'active' }, null, true)).toEqual({ tone: 'warn', label: 'Waiting for you' });
		expect(chipFor({ status: 'paused' }, null, true).label).toBe('Paused');
	});

	it('takes the pace once nothing runs or waits', () => {
		expect(chipFor({ status: 'active' }, 'done', false, 'behind')).toEqual({ tone: 'missed', label: 'Behind' });
		expect(chipFor({ status: 'active' }, null, false, 'ahead')).toEqual({ tone: 'done', label: 'Ahead' });
		expect(chipFor({ status: 'active' }, null, false, 'on_track')).toEqual({ tone: 'done', label: 'On track' });
		expect(chipFor({ status: 'draft' }, null, false, 'on_track').label).toBe('Planning');
		expect(chipFor({ status: 'active' }, 'failed', false, 'on_track').label).toBe('Failed');
	});

	it('tells an owner stop (grey Stopped) from an error or a time limit (red Failed)', () => {
		expect(runState('failed', 'cancelled')).toBe('stopped');
		expect(runState('failed', 'time_limit')).toBe('failed');
		expect(runState('failed', null)).toBe('failed');
		expect(runState('running', 'cancelled')).toBe('running');
		expect(goalChip({ status: 'draft' }, [{ ...summary('r', 'failed'), stop: 'time_limit' }], false)).toEqual({ tone: 'missed', label: 'Failed' });
		expect(goalChip({ status: 'draft' }, [{ ...summary('r', 'failed'), stop: 'cancelled' }], false)).toEqual({ tone: 'skipped', label: 'Stopped' });
	});

	it('gives the run part alone for Inbox rows, null for a finished or unknown state', () => {
		expect(runChip('failed')).toEqual({ tone: 'missed', label: 'Failed' });
		expect(runChip('done')).toBeNull();
		expect(runChip('odd')).toBeNull();
	});

	it('merges the newest runs with older waiting ones, once each, newest first', () => {
		const run = (id: string, state: string, created_at: string): RunSummary => ({ ...summary(id, state), created_at });
		const newest = [run('b', 'failed', '2026-11-25T10:00:00Z'), run('a', 'waiting_human', '2026-11-20T10:00:00Z')];
		const waiting = [run('a', 'waiting_human', '2026-11-20T10:00:00Z'), run('old', 'waiting_human', '2026-10-01T10:00:00Z')];
		expect(mergeRuns(newest, waiting).map((r) => r.run_id)).toEqual(['b', 'a', 'old']);
	});
});

describe('goal card status', () => {
	it('b70: an engine read that failed never shows On track; it says so and offers Retry', () => {
		const s = goalStatus(null, 5, 0, IDLE, true);
		expect(s.chip).not.toBe('Active');
		expect(s).toMatchObject({ tone: 'missed', chip: 'Offline', title: 'Could not reach the engine', retry: true, stop: false });
		expect(goalStatus(null, 5, 0, IDLE, false)).toMatchObject({ chip: 'Active', detail: 'No run yet.' });
	});

	it('follows the run that waits for the owner, else the newest run', () => {
		expect(pickRun([])).toBeNull();
		expect(pickRun([summary('new', 'done'), summary('old', 'waiting_human')])).toBe('old');
		expect(pickRun([summary('new', 'running'), summary('old', 'done')])).toBe('new');
	});

	it('reads research and answers from the newest new-goal run, not a later replan', () => {
		const replan = { ...summary('re', 'done'), workflow: 'replan' };
		expect(originRun([replan, summary('first', 'done')])).toBe('first');
		expect(originRun([replan])).toBeNull();
		expect(originRun([replan, { ...summary('again', 'done'), workflow: 'run-again' }, summary('first', 'done')])).toBe('again');
	});

	it('counts active step time, with an open step running until now', () => {
		const events = [at(1, 1_000, 'step_started', 'intake'), at(2, 31_000, 'step_finished', 'intake'), at(3, 3_600_000, 'step_started', 'research')];
		expect(activeMs(events, 3_612_000)).toBe(42_000);
	});

	it('says what agents do while working, with time of the limit and cost', () => {
		const running = view({ events: [at(1, 0, 'step_started', 'research')] });
		const s = goalStatus(running, 5, 42_000, shared(running));
		expect(s).toMatchObject({ tone: 'planned', chip: 'Working', title: 'Researching how to reach it', detail: '0:42 of 5:00 · $0.01 so far', stop: true, retry: false });
	});

	it('says what the run waits for, amber', () => {
		const questions = view({ status: { status: 'waiting_human' }, waiting: [{ node: 'questions', ask: 'question', prompt: null, data: { intake: { questions: [{ id: 'a' }, { id: 'b' }] } } }] });
		expect(goalStatus(questions, 5, 0, shared(questions))).toMatchObject({ tone: 'warn', chip: 'Waiting for you', title: '2 questions to answer', detail: 'The timer is paused while you answer.' });
		const plan = view({ status: { status: 'waiting_human' }, waiting: [{ node: 'approve', ask: 'approval', prompt: null, data: { plan: { items: [] } } }] });
		expect(goalStatus(plan, 5, 0, shared(plan))).toMatchObject({ tone: 'warn', title: 'Plan to approve', detail: 'Nothing is on your calendar until you approve.' });
	});

	it('b97: a time-limit stop is red Failed, an owner stop grey Stopped, each with its stop line, Retry and the money spent', () => {
		const limited = view({ status: { status: 'failed', reason: { kind: 'time_limit', limit_ms: 300_000 } } });
		const s = goalStatus(limited, 5, 0, shared(limited));
		expect(s).toMatchObject({ tone: 'missed', chip: 'Failed', title: 'Stopped after 5 min.', detail: 'Nothing more was spent: $0.01 in total.', stop: false, retry: true, limit: true });
		const stopped = view({ status: { status: 'failed', reason: { kind: 'cancelled' } } });
		expect(goalStatus(stopped, 5, 0, shared(stopped))).toMatchObject({ tone: 'skipped', chip: 'Stopped', title: 'Stopped. Nothing more runs or is spent.', retry: true, limit: false });
	});

	it('offers Retry on a run failed by a transient error, without Change time limit', () => {
		const error = 'node `write`: tool `write_plan`: failed: database: max clients reached';
		const transient = view({ status: { status: 'failed', error, retryable: true } });
		const s = goalStatus(transient, 5, 0, shared(transient));
		expect(s).toMatchObject({ tone: 'missed', chip: 'Failed', title: `The run stopped: ${error}.`, detail: '$0.01 in total.', stop: false, retry: true, limit: false });
		expect(goalStatus(view({ status: { status: 'failed', error: 'max_steps 200 reached', retryable: false } }), 5, 0, IDLE)).toMatchObject({ retry: false, limit: false });
	});

	it('shows a paused or failed run with its reason, and an idle goal with the same label as the Goals list', () => {
		const paused = view({ status: { status: 'waiting_budget', reason: { kind: 'budget', limit: 'goal' } } });
		expect(goalStatus(paused, 5, 0, shared(paused))).toMatchObject({ tone: 'warn', chip: 'Paused' });
		const failed = view({ status: { status: 'failed', error: 'boom' } });
		expect(goalStatus(failed, 5, 0, shared(failed))).toMatchObject({ tone: 'missed', chip: 'Failed', title: 'The run stopped: boom.', retry: false, limit: false });
		const pace = chipFor({ status: 'active' }, 'done', false, 'on_track');
		expect(goalStatus(view({ status: { status: 'done' } }), 5, 0, pace)).toMatchObject({ tone: 'done', chip: 'On track', title: 'Agents are idle', detail: 'Last run $0.01.' });
		const draft = chipFor({ status: 'draft' }, null, false);
		expect(goalStatus(null, 5, 0, draft)).toMatchObject({ tone: 'planned', chip: 'Planning', title: 'Agents are idle', detail: 'No run yet.' });
	});
});

describe('changed answers', () => {
	it('become one replan reason naming each question and its new answer', () => {
		expect(answersReason([{ id: 'pace', text: 'Minutes a day?', answer: '45' }, { id: 'when', text: 'When?', answer: null }])).toBe(
			'I changed my answers. Minutes a day?: 45. When?: skipped. Update the plan to fit.'
		);
	});
});

describe('run again while a plan waits', () => {
	it('is blocked only while the followed run waits for approval of a plan', () => {
		const plan = { node: 'approve', ask: 'approval' as const, prompt: null, data: { plan: { items: [], timezone: 'UTC' } } };
		expect(planWaits(null)).toBe(false);
		expect(planWaits(view({ status: { status: 'waiting_human' }, waiting: [plan] }))).toBe(true);
		expect(planWaits(view({ status: { status: 'waiting_human' }, waiting: [{ node: 'questions', ask: 'question', prompt: null, data: {} }] }))).toBe(false);
		expect(planWaits(view({ status: { status: 'done' }, waiting: [] }))).toBe(false);
		expect(planWaits(view({ status: { status: 'running' }, waiting: [plan] }))).toBe(false);
	});
});

describe('settledMark (B73)', () => {
	it('b73: polls of a live run keep one mark, so coaches and plugins are fetched once per settled run, not per poll', () => {
		const marks = (['queued', 'running', 'running', 'checkpointed', 'running', 'waiting_human', 'waiting_human', 'running', 'done'] as const).map((status) => settledMark(view({ status: { status } })));
		expect(new Set(marks.filter((m) => m !== null))).toEqual(new Set(['r:waiting_human', 'r:done']));
		expect(marks.slice(0, 5)).toEqual([null, null, null, null, null]);
		expect(settledMark(null)).toBeNull();
	});
});

describe('run marks already read at open (B87)', () => {
	it('a mark the runs read at open already holds refetches nothing; a later one does', () => {
		const opened = [summary('r2', 'done'), summary('r1', 'waiting_human')];
		expect(markRead('r2:done', opened)).toBe(true);
		expect(markRead('r1:waiting_human', opened)).toBe(true);
		expect(markRead('r1:done', opened)).toBe(false);
		expect(markRead('r3:failed', opened)).toBe(false);
		expect(markRead('r2:done', null)).toBe(false);
	});
});
