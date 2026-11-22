import { describe, expect, it, vi } from 'vitest';
import { chatFromRun, Engine, flushLogs, keyStatus, pauseMessage, route, type Answer, type Call, type RunView, type Waiting } from './engine';

vi.mock('@tauri-apps/api/core', () => ({ invoke: vi.fn() }));

const questions: Waiting = {
	node: 'questions',
	ask: 'question',
	prompt: 'Answer these so the plan fits you.',
	data: { intake: { questions: [{ id: 'days', text: 'How many days a week?', kind: 'number', choices: [] }, { id: 'food', text: 'Anything you do not eat?', kind: 'text', choices: [] }] } }
};
const approve: Waiting = {
	node: 'approve',
	ask: 'approval',
	prompt: 'Approve this plan?',
	data: { plan: { summary: 'Core work and a daily plank.', items: [{ title: 'Core workout', start: '2026-11-22T07:00:00', minutes: 30, rrule: null, notes: null }] } }
};

/** A run view at `status` waiting at `waiting`, with intake and research done. */
function view(status: RunView['status'], waiting: Waiting[] = []): RunView {
	const events = [
		{ seq: 0, at_ms: 1, body: { type: 'run_started' } },
		{ seq: 1, at_ms: 2, body: { type: 'step_started', step: 'intake' } },
		{ seq: 2, at_ms: 3, body: { type: 'step_finished', step: 'intake' } },
		{ seq: 3, at_ms: 4, body: { type: 'step_started', step: 'research' } }
	];
	return { run_id: 'r1', goal_id: 'g1', workflow: 'new-goal', status, waiting, events };
}

describe('engine client', () => {
	it('maps a run view to chat messages', () => {
		const asking = chatFromRun(view({ status: 'waiting_human' }, [questions]));
		expect(asking[0]).toEqual({ type: 'progress', lines: [{ text: 'Reading your goal', done: true }, { text: 'Researching how to reach it', done: false }] });
		const cards = asking.filter((m) => m.type === 'card');
		expect(cards).toHaveLength(2);
		expect(cards[0]).toMatchObject({ spec: { kind: 'question_card', question: 'How many days a week?', input: { kind: 'number_input' } } });
		const plan = chatFromRun(view({ status: 'waiting_human' }, [approve])).at(-1);
		expect(plan).toEqual({ type: 'plan', title: 'Core work and a daily plank.', items: ['Core workout, 2026-11-22 07:00, 30 min'] });
		expect(chatFromRun(view({ status: 'done' })).at(-1)).toMatchObject({ text: 'Approved. The plan is on your calendar.' });
	});

	it('shows a clear line for budget pauses and failures', () => {
		expect(pauseMessage({ status: 'waiting_budget', reason: { kind: 'budget', limit: 'unverified_price' } })).toContain('price is not verified');
		expect(pauseMessage({ status: 'waiting_budget', reason: { kind: 'budget', limit: 'user' } })).toBe('Paused: the user budget is reached. Raise it in Settings to continue.');
		expect(pauseMessage({ status: 'failed', error: 'no key' })).toBe('The run stopped: no key.');
		expect(pauseMessage({ status: 'checkpointed' })).toBeNull();
		expect(chatFromRun(view({ status: 'waiting_budget', reason: { kind: 'budget', limit: 'unverified_price' } })).at(-1)).toMatchObject({ from: 'persona', name: 'Aroh' });
	});

	it('routes chat text to the waiting node or a replan', () => {
		expect(route(undefined, [], 'I missed today')).toEqual({ kind: 'replan', reason: 'I missed today' });
		expect(route(questions, [], '4')).toEqual({ kind: 'hold', held: ['4'] });
		expect(route(questions, ['4'], 'no nuts')).toEqual({
			kind: 'answer',
			node: 'questions',
			answer: [
				{ id: 'days', answer: '4' },
				{ id: 'food', answer: 'no nuts' }
			],
			held: []
		});
		expect(route(approve, [], 'Mornings only')).toEqual({ kind: 'answer', node: 'approve', answer: false, held: ['Mornings only'] });
		expect(route({ node: 'change', ask: 'question', prompt: null, data: {} }, [], 'Mornings only')).toEqual({ kind: 'answer', node: 'change', answer: 'Mornings only', held: [] });
	});

	it('sendChat answers the waiting run, else starts a replan', async () => {
		const sent: [string, string, unknown][] = [];
		const answers: Record<string, Answer> = {
			'GET /v1/runs?goal_id=g1&state=waiting_human': { status: 200, body: [{ run_id: 'r1' }] },
			'GET /v1/runs/r1': { status: 200, body: view({ status: 'waiting_human' }, [approve]) },
			'POST /v1/runs/r1/answers': { status: 202, body: { run_id: 'r1' } },
			'GET /v1/runs?goal_id=g2&state=waiting_human': { status: 200, body: [] },
			'POST /v1/goals/g2/replan': { status: 202, body: { goal_id: 'g2', run_id: 'r2' } }
		};
		const call: Call = async (method, path, body) => {
			sent.push([method, path, body]);
			return answers[`${method} ${path}`] ?? { status: 404, body: { error: { code: 'not_found', message: 'no such route' } } };
		};
		const e = new Engine(call);
		await expect(e.sendChat('g1', 'Evenings please')).resolves.toEqual({ ok: true, value: { run_id: 'r1', held: ['Evenings please'] } });
		expect(sent.find(([m, p]) => m === 'POST' && p === '/v1/runs/r1/answers')?.[2]).toEqual({ node: 'approve', answer: false });
		await expect(e.sendChat('g2', 'I missed today')).resolves.toEqual({ ok: true, value: { run_id: 'r2', held: [] } });
		expect(sent.at(-1)?.[2]).toMatchObject({ reason: 'I missed today' });
		await expect(e.answer('nope', 'approve', true)).resolves.toEqual({ ok: false, reason: 'error', message: 'no such route' });
	});

	it('maps a missing session or server URL to needs_engine', async () => {
		const e = new Engine(() => Promise.reject('needs_engine: sign in first'));
		await expect(e.startGoal('six pack by Feb 1')).resolves.toMatchObject({ ok: false, reason: 'needs_engine' });
	});

	it('reads insights, optionally of one goal', async () => {
		const calls: string[] = [];
		const e = new Engine((method, path) => {
			calls.push(`${method} ${path}`);
			return Promise.resolve({ status: 200, body: [] });
		});
		await expect(e.insights()).resolves.toEqual({ ok: true, value: [] });
		await e.insights('g1');
		expect(calls).toEqual(['GET /v1/insights', 'GET /v1/insights?goal_id=g1']);
	});

	it('flushes queued logs in order and stops at the first failure', async () => {
		const pending = [
			{ text: 'had pasta', at: '2026-11-22T08:00:00Z' },
			{ text: 'salad', at: '2026-11-22T12:00:00Z' },
			{ text: 'cake', at: '2026-11-22T15:00:00Z' }
		];
		const posted: unknown[] = [];
		const call: Call = async (_m, _p, body) => {
			posted.push(body);
			return posted.length === 2 ? { status: 500, body: null } : { status: 202, body: { goal_id: null, run_id: 'r9' } };
		};
		await expect(flushLogs(call, pending, 'Europe/Berlin')).resolves.toEqual({ sent: [pending[0]], runs: ['r9'] });
		expect(posted).toEqual([
			{ text: 'had pasta', at: '2026-11-22T08:00:00Z', timezone: 'Europe/Berlin' },
			{ text: 'salad', at: '2026-11-22T12:00:00Z', timezone: 'Europe/Berlin' }
		]);
		await expect(flushLogs(() => Promise.reject('offline'), pending, 'UTC')).resolves.toEqual({ sent: [], runs: [] });
	});

	it('shows key status per provider row, vendor and last 4 only', () => {
		const keys = [{ setting_id: 'p1', vendor: 'anthropic', last4: '4321', updated_at: '2026-11-22T00:00:00Z' }];
		const providers = [
			{ id: 'p1', vendor: 'anthropic' },
			{ id: 'p2', vendor: 'openai' }
		];
		expect(keyStatus(keys, providers)).toEqual({ anthropic: '4321', openai: null });
		expect(keyStatus([{ ...keys[0], setting_id: 'other' }], providers)).toEqual({ anthropic: null, openai: null });
	});
});
