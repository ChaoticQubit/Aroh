import { describe, expect, it } from 'vitest';
import { activeMs, answersReason, goalStatus, originRun, pickRun } from './goalcard';
import type { RunEvent, RunSummary, RunView } from './engine';

const at = (seq: number, ms: number, type: string, step: string): RunEvent => ({ seq, at_ms: ms, body: { type, step } });
const summary = (run_id: string, state: string): RunSummary => ({ run_id, goal_id: 'g', workflow: 'new-goal', state, created_at: '', updated_at: '' });
const view = (patch: Partial<RunView>): RunView => ({ run_id: 'r', goal_id: 'g', workflow: 'new-goal', status: { status: 'running' }, waiting: [], events: [], usage: { cost_usd_micros: 10_000 }, ...patch });

describe('goal card status', () => {
	it('follows the run that waits for the owner, else the newest run', () => {
		expect(pickRun([])).toBeNull();
		expect(pickRun([summary('new', 'done'), summary('old', 'waiting_human')])).toBe('old');
		expect(pickRun([summary('new', 'running'), summary('old', 'done')])).toBe('new');
	});

	it('reads research and answers from the newest new-goal run, not a later replan', () => {
		const replan = { ...summary('re', 'done'), workflow: 'replan' };
		expect(originRun([replan, summary('first', 'done')])).toBe('first');
		expect(originRun([replan])).toBeNull();
	});

	it('counts active step time, with an open step running until now', () => {
		const events = [at(1, 1_000, 'step_started', 'intake'), at(2, 31_000, 'step_finished', 'intake'), at(3, 3_600_000, 'step_started', 'research')];
		expect(activeMs(events, 3_612_000)).toBe(42_000);
	});

	it('says what agents do while working, with time of the limit and cost', () => {
		const s = goalStatus(view({ events: [at(1, 0, 'step_started', 'research')] }), 5, 42_000);
		expect(s).toMatchObject({ tone: 'planned', chip: 'Working', title: 'Researching how to reach it', detail: '0:42 of 5:00 · $0.01 so far', stop: true, retry: false });
	});

	it('says what the run waits for, amber', () => {
		const questions = view({ status: { status: 'waiting_human' }, waiting: [{ node: 'questions', ask: 'question', prompt: null, data: { intake: { questions: [{ id: 'a' }, { id: 'b' }] } } }] });
		expect(goalStatus(questions, 5, 0)).toMatchObject({ tone: 'warn', chip: 'Waiting for you', title: '2 questions to answer', detail: 'The timer is paused while you answer.' });
		const plan = view({ status: { status: 'waiting_human' }, waiting: [{ node: 'approve', ask: 'approval', prompt: null, data: { plan: { items: [] } } }] });
		expect(goalStatus(plan, 5, 0)).toMatchObject({ tone: 'warn', title: 'Plan to approve', detail: 'Nothing is on your calendar until you approve.' });
	});

	it('shows a stopped run grey with Retry and the money spent', () => {
		const s = goalStatus(view({ status: { status: 'failed', reason: { kind: 'time_limit', limit_ms: 300_000 } } }), 5, 0);
		expect(s).toMatchObject({ tone: 'skipped', chip: 'Stopped', title: 'Stopped after 5 min.', detail: 'Nothing more was spent: $0.01 in total.', stop: false, retry: true });
	});

	it('shows a paused or failed run with its reason, and a finished goal on track', () => {
		expect(goalStatus(view({ status: { status: 'waiting_budget', reason: { kind: 'budget', limit: 'goal' } } }), 5, 0)).toMatchObject({ tone: 'warn', chip: 'Paused' });
		expect(goalStatus(view({ status: { status: 'failed', error: 'boom' } }), 5, 0)).toMatchObject({ tone: 'missed', chip: 'Failed', title: 'The run stopped: boom.', retry: false });
		expect(goalStatus(view({ status: { status: 'done' } }), 5, 0)).toMatchObject({ tone: 'done', chip: 'On track', title: 'Agents are idle', detail: 'Last run $0.01.' });
		expect(goalStatus(null, 5, 0)).toMatchObject({ tone: 'done', chip: 'On track', title: 'Agents are idle', detail: 'No run yet.' });
	});
});

describe('changed answers', () => {
	it('become one replan reason naming each question and its new answer', () => {
		expect(answersReason([{ id: 'pace', text: 'Minutes a day?', answer: '45' }, { id: 'when', text: 'When?', answer: null }])).toBe(
			'I changed my answers. Minutes a day?: 45. When?: skipped. Update the plan to fit.'
		);
	});
});
