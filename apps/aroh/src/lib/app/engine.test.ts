import { describe, expect, it, vi } from 'vitest';
import { chatFromRun, Engine, fixedActionIds, flushLogs, keyStatus, NEEDS_ENGINE, ENGINE_PROBLEM, pauseMessage, progressLines, route, unreachable, waitLabel, type Answer, type Call, type RunView, type Waiting } from './engine';
import type { Action } from './client';
import { deviceZone } from '$lib/ui/clock';

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
		expect(asking[0]).toEqual({ type: 'progress', working: false, lines: [{ text: 'Reading your goal', done: true }, { text: 'Researching how to reach it', done: false }] });
		expect(chatFromRun(view({ status: 'failed', reason: { kind: 'cancelled' } }))[0]).toMatchObject({ type: 'progress', working: false });
		expect(chatFromRun(view({ status: 'running' }))[0]).toMatchObject({ type: 'progress', working: true });
		expect(asking.filter((m) => m.type === 'card')).toHaveLength(0);
		expect(waitLabel(view({ status: 'waiting_human' }, [questions]))).toBe('2 questions to answer');
		expect(waitLabel(view({ status: 'waiting_human' }, [approve]))).toBe('Plan to approve');
		expect(waitLabel(view({ status: 'waiting_human' }, [{ node: 'change', ask: 'question', prompt: null, data: {} }]))).toBe('What to change in the plan');
		expect(waitLabel(view({ status: 'running' }))).toBeNull();
		const plan = chatFromRun(view({ status: 'waiting_human' }, [approve])).at(-1);
		expect(plan).toEqual({ type: 'plan', title: 'Core work and a daily plank.', items: ['Core workout, 2026-11-22 07:00, 30 min'], fixable: [true], plan: { items: [{ title: 'Core workout', start: '2026-11-22T07:00:00', minutes: 30, rrule: null }], timezone: null }, key: 'r1:approve', approved: false });
		expect(chatFromRun(view({ status: 'done' })).at(-1)).toMatchObject({ text: 'Approved. The plan is on your calendar.' });
	});

	it('finds the actions of the plan items the owner marked Fixed time', () => {
		const item = (title: string, start: string, rrule: string | null = null) => ({ title, start, minutes: 30, rrule, notes: null });
		const items = [item('Dentist', '2026-11-24T09:00:00'), item('Run', '2026-11-23T07:00:00', 'FREQ=DAILY'), item('Dentist', '2026-11-25T09:00:00'), item('Gone', '2026-11-26T09:00:00')];
		const row = (id: string, title: string, start_at: string, extra: Partial<Action> = {}): Action => ({
			id,
			goal_id: 'g1',
			series_id: null,
			occurrence: null,
			title,
			notes: null,
			start_at,
			end_at: start_at,
			status: 'planned',
			locked: false,
			done_at: null,
			...extra
		});
		const actions = [
			row('a1', 'Dentist', '2026-11-24T08:00:00Z'),
			row('a2', 'Dentist', '2026-11-25T08:00:00Z'),
			row('a3', 'Run', '2026-11-23T06:00:00Z', { series_id: 's1' }),
			row('a4', 'Dentist', '2026-11-25T08:00:00Z', { goal_id: 'g2' })
		];
		expect(fixedActionIds(actions, 'g1', items, [2, 1, 3], 'Europe/Berlin')).toEqual(['a2']);
		expect(fixedActionIds(actions, 'g1', items, [0], 'UTC'), 'local start must match in the plan zone').toEqual([]);
	});

	it('shows a clear line for budget pauses and failures', () => {
		expect(pauseMessage({ status: 'waiting_budget', reason: { kind: 'budget', limit: 'unverified_price' } })).toContain('price is not verified');
		expect(pauseMessage({ status: 'waiting_budget', reason: { kind: 'budget', limit: 'user' } })).toBe('Paused: the user budget is reached. Raise it in Settings to continue.');
		expect(pauseMessage({ status: 'failed', error: 'no key' })).toBe('The run stopped: no key.');
		const clash = 'node `place_check`: tool `check_overlap`: blocked: Could not fit sessions around your calendar: Gym: Upper body A clashes with Gym: Full body A (legs, chest, back)';
		expect(pauseMessage({ status: 'failed', error: clash })).toBe('Could not fit sessions around your calendar: Gym: Upper body A clashes with Gym: Full body A (legs, chest, back).');
		expect(pauseMessage({ status: 'failed', error: 'node `plan`: BadRequest: The plan was too long to finish' })).toBe('The plan was too long to finish.');
		const events = [{ seq: 1, at_ms: 0, body: { type: 'step_started', step: 'place' } }];
		expect(progressLines(events)).toEqual([{ text: 'Moving sessions around your calendar', done: false }]);
		expect(pauseMessage({ status: 'checkpointed' })).toBeNull();
		expect(chatFromRun(view({ status: 'waiting_budget', reason: { kind: 'budget', limit: 'unverified_price' } })).at(-1)).toMatchObject({ from: 'persona', name: 'Aroh' });
	});

	it('routes chat text to the waiting node or a replan', () => {
		expect(route(undefined, 'I missed today')).toEqual({ kind: 'replan', reason: 'I missed today' });
		expect(route(questions, '4')).toEqual({ kind: 'questions' });
		const limits: Waiting = { node: 'limit_choice', ask: 'question', prompt: 'Pick what should change.', data: { limits: { questions: [{ id: 'limit_choice', text: 'What should change?', kind: 'choice', choices: ['Lower the target'] }] } } };
		expect(route(limits, 'lower it')).toEqual({ kind: 'questions' });
		expect(waitLabel(view({ status: 'waiting_human' }, [limits]))).toBe('1 question to answer');
		expect(route(approve, 'Mornings only')).toEqual({ kind: 'answer', node: 'approve', answer: false, change: 'Mornings only' });
		expect(route({ node: 'change', ask: 'question', prompt: null, data: {} }, 'Mornings only')).toEqual({ kind: 'answer', node: 'change', answer: 'Mornings only' });
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
		await expect(e.sendChat('g1', 'Evenings please')).resolves.toEqual({ ok: true, value: { run_id: 'r1', change: 'Evenings please' } });
		expect(sent.find(([m, p]) => m === 'POST' && p === '/v1/runs/r1/answers')?.[2]).toEqual({ node: 'approve', answer: false });
		await expect(e.sendChat('g2', 'I missed today')).resolves.toEqual({ ok: true, value: { run_id: 'r2' } });
		expect(sent.at(-1)?.[2]).toMatchObject({ reason: 'I missed today' });
		await expect(e.answer('nope', 'approve', true)).resolves.toEqual({ ok: false, reason: 'error', message: 'no such route' });
	});

	it('maps a missing session or server URL to needs_engine', async () => {
		const e = new Engine(() => Promise.reject('needs_engine: sign in first'));
		await expect(e.startGoal({ title: 'six pack by Feb 1', autonomy: 'balanced', reminders: true, use_calendars: true, timezone: 'UTC' })).resolves.toMatchObject({ ok: false, reason: 'needs_engine' });
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

	it('lists, copies and edits library personas', async () => {
		const safety = 'Safety: no individual investment, tax or legal advice; refer to a licensed financial professional for those.';
		const coach = { name: 'budget-coach', display_name: 'Budget coach', source: 'library' };
		const fields = { display_name: 'Money mentor', role: 'Warm.', instructions: 'Say "hi"\nthen go.', guardrails: [safety] };
		const sent: [string, string, unknown][] = [];
		const answers: Record<string, Answer> = {
			'GET /v1/personas': { status: 200, body: [coach] },
			'POST /v1/personas/budget-coach/copy': { status: 201, body: coach },
			'POST /v1/personas/tutor/copy': { status: 409, body: { error: { code: 'conflict', message: 'conflict: persona tutor is already in the library' } } },
			'PUT /v1/personas/budget-coach': { status: 200, body: { ...coach, display_name: 'Money mentor' } },
			'PUT /v1/personas/tutor': { status: 422, body: { error: { code: 'setup', message: `setup: invalid_persona: tutor.toml: guardrails: keep \`${safety}\`` } } },
			'PUT /v1/personas/habit-coach': { status: 422, body: { error: { code: 'setup', message: 'setup: invalid_persona: habit-coach.toml: role: must not be blank' } } }
		};
		const e = new Engine(async (method, path, body) => {
			sent.push([method, path, body]);
			return answers[`${method} ${path}`] ?? { status: 404, body: { error: { code: 'not_found', message: 'not found: library persona' } } };
		});
		await expect(e.personas()).resolves.toEqual({ ok: true, value: [coach] });
		await expect(e.copyPersona('budget-coach')).resolves.toEqual({ ok: true, value: coach });
		await expect(e.copyPersona('tutor')).resolves.toMatchObject({ ok: false, reason: 'exists' });
		await expect(e.editPersona('budget-coach', fields)).resolves.toMatchObject({ ok: true, value: { display_name: 'Money mentor' } });
		expect(sent.at(-1)).toEqual(['PUT', '/v1/personas/budget-coach', fields]);
		await expect(e.editPersona('tutor', fields)).resolves.toEqual({ ok: false, reason: 'error', message: `Safety rules stay on every coach: ${safety}` });
		await expect(e.editPersona('habit-coach', fields)).resolves.toEqual({ ok: false, reason: 'error', message: 'habit-coach.toml: role: must not be blank' });
		await expect(e.editPersona('nobody', fields)).resolves.toMatchObject({ ok: false, reason: 'error' });
		const offline = new Engine(() => Promise.reject('needs_engine: sign in first'));
		await expect(offline.personas()).resolves.toEqual({ ok: false, reason: 'needs_engine', message: NEEDS_ENGINE });
		await expect(offline.copyPersona('tutor')).resolves.toEqual({ ok: false, reason: 'needs_engine', message: NEEDS_ENGINE });
		await expect(offline.editPersona('tutor', fields)).resolves.toEqual({ ok: false, reason: 'needs_engine', message: NEEDS_ENGINE });
	});
});


describe('run limits (Q60)', () => {
	it('a working run shows its running step, cost so far and a Stop', () => {
		const working = { ...view({ status: 'running' }), usage: { cost_usd_micros: 12_345 } };
		expect(chatFromRun(working).at(-1)).toEqual({ type: 'working', step: 'Researching how to reach it', cost: '$0.01' });
	});

	it('a run stopped at its time limit says after how long, with Retry; a cancelled one says stopped', () => {
		const limit = { status: 'failed' as const, error: 'time limit of 300000 ms reached', reason: { kind: 'time_limit', limit_ms: 300_000 } };
		expect(chatFromRun(view(limit)).at(-1)).toEqual({ type: 'stopped', text: 'Stopped after 5 min.', retry: true });
		const cancelled = { status: 'failed' as const, error: 'cancelled by the owner', reason: { kind: 'cancelled' } };
		expect(chatFromRun(view(cancelled)).at(-1)).toEqual({ type: 'stopped', text: 'Stopped. Nothing more runs or is spent.', retry: true });
		expect(chatFromRun(view({ status: 'failed', error: 'no key' })).at(-1)).toMatchObject({ type: 'message', text: 'The run stopped: no key.' });
	});

	it('a run failed by a transient error offers Retry in the chat; a final failure does not', () => {
		const transient = { status: 'failed' as const, error: 'node `write`: tool `write_plan`: failed: database: busy', retryable: true };
		expect(chatFromRun(view(transient)).at(-1)).toEqual({ type: 'stopped', text: 'The run stopped: node `write`: tool `write_plan`: failed: database: busy.', retry: true });
		expect(chatFromRun(view({ ...transient, retryable: false })).at(-1)).toMatchObject({ type: 'message', from: 'persona' });
	});

	it('runs a goal again with the saved answers, or with the edited ones', async () => {
		const sent: string[] = [];
		const e = new Engine(async (method, path, body) => {
			sent.push(`${method} ${path} ${JSON.stringify(body ?? null)}`);
			return { status: 202, body: { goal_id: 'g1', run_id: 'r2' } };
		});
		await expect(e.runAgain('g1')).resolves.toEqual({ ok: true, value: { goal_id: 'g1', run_id: 'r2' } });
		await e.runAgain('g1', [{ id: 'pace', text: 'Minutes a day?', answer: '45' }, { id: 'when', text: 'When?', answer: null }]);
		const zone = JSON.stringify(deviceZone());
		expect(sent).toEqual([
			`POST /v1/goals/g1/run-again {"timezone":${zone}}`,
			`POST /v1/goals/g1/run-again {"timezone":${zone},"answers":[{"id":"pace","answer":"45"},{"id":"when","answer":null}]}`
		]);
	});

	it('says a finished run again is on the calendar, like a new goal', () => {
		expect(chatFromRun({ ...view({ status: 'done' }), workflow: 'run-again' }).at(-1)).toMatchObject({ text: 'Approved. The plan is on your calendar.' });
	});

	it('cancels, retries and reads and saves the run time limit', async () => {
		const sent: string[] = [];
		const e = new Engine(async (method, path, body) => {
			sent.push(`${method} ${path} ${JSON.stringify(body ?? null)}`);
			return { status: 200, body: path.endsWith('run-limit') ? { minutes: 5 } : { run_id: 'r1' } };
		});
		await expect(e.cancel('r1')).resolves.toMatchObject({ ok: true });
		await expect(e.retry('r1')).resolves.toMatchObject({ ok: true });
		await expect(e.runLimit()).resolves.toEqual({ ok: true, value: { minutes: 5 } });
		await e.setRunLimit(8);
		expect(sent).toEqual(['POST /v1/runs/r1/cancel null', 'POST /v1/runs/r1/retry null', 'GET /v1/settings/run-limit null', 'PUT /v1/settings/run-limit {"minutes":8}']);
	});

	it('records AI consent and deletes the account under /v1/account (Q76)', async () => {
		const sent: string[] = [];
		let consentAt: string | null = null;
		const e = new Engine(async (method, path, body) => {
			sent.push(`${method} ${path} ${JSON.stringify(body ?? null)}`);
			if (path === '/v1/account' && method === 'GET') return { status: 200, body: { ai_consent_at: consentAt, open_goals: 0 } };
			if (path === '/v1/account') return { status: 202, body: { purge_after: '2026-12-08T10:00:00Z' } };
			return { status: 204, body: null };
		});
		await expect(e.consent()).resolves.toEqual({ ok: true, value: false });
		consentAt = '2026-11-23T10:00:00Z';
		await expect(e.consent()).resolves.toEqual({ ok: true, value: true });
		await expect(e.giveConsent()).resolves.toEqual({ ok: true, value: true });
		await expect(e.deleteAccount()).resolves.toEqual({ ok: true, value: { purge_after: '2026-12-08T10:00:00Z' } });
		expect(sent).toEqual(['GET /v1/account null', 'GET /v1/account null', 'PUT /v1/account/consent {"ai":true}', 'DELETE /v1/account null']);
		const old = new Engine(async () => ({ status: 404, body: null }));
		await expect(old.giveConsent()).resolves.toEqual({ ok: true, value: false });
	});
});

describe('unreachable (B69)', () => {
	it('b69: a failed engine read is unreachable, while no engine set up or a good read is not', () => {
		expect(unreachable({ ok: false, reason: 'error', message: 'The engine answered 500.' })).toBe(true);
		expect(unreachable({ ok: false, reason: 'needs_engine', message: 'Set up the engine.' })).toBe(false);
		expect(unreachable({ ok: true, value: [] })).toBe(false);
	});
});

describe('engine reads in flight (B87)', () => {
	it('one GET in flight is shared by every caller of the same path; writes and later reads go out again', async () => {
		const sent: string[] = [];
		let release: () => void = () => {};
		const gate = new Promise<void>((r) => (release = r));
		const call: Call = async (method, path) => {
			sent.push(`${method} ${path}`);
			await gate;
			return { status: 200, body: [] };
		};
		const e = new Engine(call);
		const both = Promise.all([e.runs({ goalId: 'g1' }), e.runs({ goalId: 'g1' }), e.runs({ goalId: 'g2' })]);
		void e.cancel('r1');
		void e.cancel('r1');
		release();
		const [a, b] = await both;
		expect(a).toEqual(b);
		expect(sent).toEqual(['GET /v1/runs?goal_id=g1', 'GET /v1/runs?goal_id=g2', 'POST /v1/runs/r1/cancel', 'POST /v1/runs/r1/cancel']);
		await e.runs({ goalId: 'g1' });
		expect(sent.filter((x) => x === 'GET /v1/runs?goal_id=g1')).toHaveLength(2);
	});
});

describe('engine failures in plain words (B96)', () => {
	it('b96: a 5xx or a body that is not the engine error JSON is one plain line; a 4xx keeps the engine message', async () => {
		const answers: Answer[] = [
			{ status: 500, body: { error: { code: 'internal', message: 'internal error' } } },
			{ status: 500, body: null },
			{ status: 502, body: '<html>500: INTERNAL_SERVER_ERROR</html>' },
			{ status: 404, body: null },
			{ status: 404, body: { error: { code: 'not_found', message: 'not found: goal g1' } } }
		];
		const messages: string[] = [];
		for (const a of answers) {
			const r = await new Engine(async () => a).runs({ goalId: 'g1' });
			messages.push(r.ok ? 'ok' : r.message);
		}
		expect(messages).toEqual([ENGINE_PROBLEM, ENGINE_PROBLEM, ENGINE_PROBLEM, ENGINE_PROBLEM, 'not found: goal g1']);
		for (const m of messages.slice(0, 4)) expect(m).not.toMatch(/\d{3}|internal|html/i);
	});
});
