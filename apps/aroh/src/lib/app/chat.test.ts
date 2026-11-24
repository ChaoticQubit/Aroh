import { beforeEach, describe, expect, it, vi } from 'vitest';
import type { ChatMessage } from '$lib/ui/contract';

const fake = vi.hoisted(() => ({
	latencyMs: 0,
	posts: [] as { path: string; body: unknown }[],
	questions: [] as unknown[],
	node: 'questions',
	card: null as unknown,
	answerStatus: 202,
	runsStatus: 200
}));

vi.mock('@tauri-apps/api/core', () => ({
	invoke: vi.fn(async (_cmd: string, args: { method: string; path: string; body: unknown }) => {
		await new Promise((r) => setTimeout(r, fake.latencyMs));
		const { method, path, body } = args;
		if (method === 'GET' && path === '/v1/runs?goal_id=g1&state=waiting_human') return { status: 200, body: [{ run_id: 'r1' }] };
		if (method === 'GET' && path === '/v1/runs?goal_id=g1')
			return fake.runsStatus === 200 ? { status: 200, body: [{ run_id: 'r1', goal_id: 'g1', workflow: 'run-again', state: 'waiting_human', created_at: '', updated_at: '' }] } : { status: fake.runsStatus, body: null };
		if (method === 'GET' && path.startsWith('/v1/runs/r1'))
			return {
				status: 200,
				body: {
					run_id: 'r1',
					goal_id: 'g1',
					workflow: 'new-goal',
					status: { status: 'waiting_human', reason: { kind: 'human', node: fake.node } },
					waiting: [
						fake.node === 'approve'
							? { node: 'approve', ask: 'approval', prompt: 'Approve this plan?', data: { plan: { summary: 'One long core session', timezone: 'UTC', items: [{ title: 'Long core session', start: '2026-12-10T18:00:00', minutes: 90, rrule: null }] } } }
							: fake.node === 'limit_choice'
							? { node: 'limit_choice', ask: 'question', prompt: 'Pick what should change.', data: { limits: { fits: false, questions: fake.questions, card: fake.card } } }
							: { node: 'questions', ask: 'question', prompt: 'Answer these so the plan fits you.', data: { intake: { questions: fake.questions } } }
					],
					events: [{ seq: 0, at_ms: 1, body: { type: 'run_started' } }, ...(fake.node === 'approve' ? fake.posts.map((_, i) => ({ seq: i + 1, at_ms: i + 2, body: { type: 'node_finished', node: 'change' } })) : [])],
					usage: { cost_usd_micros: 1200 }
				}
			};
		if (method === 'POST' && (path === '/v1/runs/r1/cancel' || path === '/v1/runs/r1/retry')) {
			fake.posts.push({ path, body });
			return { status: 200, body: { run_id: 'r1' } };
		}
		if (method === 'POST' && path === '/v1/runs/r1/answers') {
			fake.posts.push({ path, body });
			return fake.answerStatus === 202 ? { status: 202, body: { run_id: 'r1' } } : { status: fake.answerStatus, body: { error: { code: 'conflict', message: 'conflict: answer does not match `answers`' } } };
		}
		return { status: 404, body: { error: { code: 'not_found', message: `no route ${method} ${path}` } } };
	})
}));
vi.mock('./client', () => ({ sync: vi.fn(async () => null) }));
vi.mock('./realtime', () => ({ connectWakeups: vi.fn(async () => {}), wakeups: { wait: vi.fn(async () => 'poll') } }));

const { GoalChat } = await import('./chat.svelte');

/** The six intake questions of the owner's stuck production run (run_checkpoints step 2), old shape: no unit, min or max. */
const PROD = [
	{ id: 'pages_remaining', kind: 'number', text: 'About how many pages are left for you to read in The Joy of X?', choices: [] },
	{ id: 'daily_reading_minutes', kind: 'number', text: 'On a typical day this week, how many minutes can you set aside for reading?', choices: [] },
	{ id: 'preferred_reading_time', kind: 'choice', text: 'When do you prefer to read?', choices: ['Early morning', 'Midday', 'Evening', 'Late night', 'It varies'] },
	{ id: 'reading_pace', kind: 'number', text: 'Roughly how many pages do you read per hour?', choices: [] },
	{ id: 'book_format', kind: 'choice', text: 'What format are you reading the book in?', choices: ['Print book', 'E-reader', 'Phone or tablet', 'Audiobook'] },
	{ id: 'busy_days_limits', kind: 'text', text: 'Are there days this week when you will be unavailable or have limits like eye strain or travel? Please describe.', choices: [] }
];

const TWO = [
	{ id: 'days', kind: 'number', text: 'How many days a week?', choices: [] },
	{ id: 'food', kind: 'text', text: 'Anything you do not eat?', choices: [] }
];

/** Lets every pending fake call and follow-up settle. */
async function settle() {
	for (let i = 0; i < 20; i++) await new Promise((r) => setTimeout(r, Math.max(1, fake.latencyMs)));
}

/** Every text the owner can read in the thread: messages, question texts and group status lines. */
function texts(messages: ChatMessage[]): string[] {
	return messages.flatMap((m) => {
		if (m.type === 'message') return [m.text];
		if (m.type === 'questions') return [m.status, ...(m.error ? [m.error] : [])];
		return [];
	});
}

/** The question group of the thread, if one renders. */
function group(messages: ChatMessage[]) {
	const g = messages.find((m) => m.type === 'questions');
	return g?.type === 'questions' ? g : undefined;
}

/** An opened goal chat with its waiting run picked up. */
async function opened(): Promise<InstanceType<typeof GoalChat>> {
	const chat = new GoalChat('g1', 'What changed?');
	await chat.resume();
	await settle();
	return chat;
}

beforeEach(() => {
	fake.latencyMs = 0;
	fake.posts = [];
	fake.questions = PROD;
	fake.node = 'questions';
	fake.answerStatus = 202;
	fake.runsStatus = 200;
});

describe('limits card (Q64)', () => {
	it('shows the limit questions and posts the pick to limit_choice', async () => {
		fake.node = 'limit_choice';
		fake.card = { cap: 20, today: '2026-11-23', target: '2026-12-05', finish: '2026-12-15', needed: 440, raise_to: 40, fits_minutes: 240 };
		fake.questions = [
			{ id: 'limit_choice', text: 'At 20 min/day you finish on Dec 15, 10 days after your target. What should change?', kind: 'choice', choices: ['Move the date to Dec 15', 'Raise the daily minutes', 'Lower the target'] },
			{ id: 'new_minutes', text: 'If you raise them, how many minutes a day?', kind: 'number', choices: [], unit: 'minutes per day', min: 21, max: 960 }
		];
		const chat = await opened();
		expect(group(chat.messages)).toMatchObject({ persona: 'Aroh', node: 'limit_choice', card: fake.card, items: [{ id: 'limit_choice' }, { id: 'new_minutes' }] });
		chat.handle({ type: 'answer', id: 'limit_choice', value: 'Move the date to Dec 15' });
		chat.handle({ type: 'skip', id: 'new_minutes' });
		chat.handle({ type: 'submit' });
		await settle();
		expect(fake.posts.map((p) => p.body)).toEqual([{ node: 'limit_choice', answer: [{ id: 'limit_choice', answer: 'Move the date to Dec 15' }, { id: 'new_minutes', answer: null }] }]);
	});
});

describe('stuck answers (Q58): the owner answered some cards and nothing was posted', () => {
	it('prod fixture: three of six answered posts nothing and says three are still open', async () => {
		const chat = await opened();
		chat.handle({ type: 'answer', id: 'pages_remaining', value: 180 });
		chat.handle({ type: 'answer', id: 'daily_reading_minutes', value: 30 });
		chat.handle({ type: 'answer', id: 'reading_pace', value: 40 });
		chat.handle({ type: 'submit' });
		await settle();
		expect(fake.posts).toEqual([]);
		expect(texts(chat.messages).join('\n')).toMatch(/3 still open/);
		expect(group(chat.messages)?.missing).toEqual(['preferred_reading_time', 'book_format', 'busy_days_limits']);
	});

	it('prod fixture: three answered and three skipped posts all six once, skips as null', async () => {
		const chat = await opened();
		chat.handle({ type: 'answer', id: 'pages_remaining', value: 180 });
		chat.handle({ type: 'answer', id: 'daily_reading_minutes', value: 30 });
		chat.handle({ type: 'answer', id: 'reading_pace', value: 40 });
		for (const id of ['preferred_reading_time', 'book_format', 'busy_days_limits']) chat.handle({ type: 'skip', id });
		expect(group(chat.messages)).toMatchObject({ missing: [], status: '3 of 6 answered, 3 skipped. Ready to send.' });
		chat.handle({ type: 'submit' });
		chat.handle({ type: 'submit' });
		await settle();
		expect(fake.posts.map((p) => p.body)).toEqual([
			{
				node: 'questions',
				answer: [
					{ id: 'pages_remaining', answer: '180' },
					{ id: 'daily_reading_minutes', answer: '30' },
					{ id: 'preferred_reading_time', answer: null },
					{ id: 'reading_pace', answer: '40' },
					{ id: 'book_format', answer: null },
					{ id: 'busy_days_limits', answer: null }
				]
			}
		]);
		expect(chat.messages.filter((m) => m.type === 'questions')).toMatchObject([{ submitted: true }]);
	});

	it('skipping every question is allowed and posts six nulls', async () => {
		const chat = await opened();
		for (const q of PROD) chat.handle({ type: 'skip', id: q.id });
		chat.handle({ type: 'submit' });
		await settle();
		expect(fake.posts.map((p) => p.body)).toEqual([{ node: 'questions', answer: PROD.map((q) => ({ id: q.id, answer: null })) }]);
	});

	it('answers sent while earlier sends are in flight all reach the one submit', async () => {
		fake.questions = TWO;
		const chat = await opened();
		fake.latencyMs = 5;
		chat.handle({ type: 'answer', id: 'days', value: 4 });
		chat.handle({ type: 'answer', id: 'food', value: 'no nuts' });
		chat.handle({ type: 'submit' });
		await settle();
		expect(fake.posts.map((p) => p.body)).toEqual([{ node: 'questions', answer: [{ id: 'days', answer: '4' }, { id: 'food', answer: 'no nuts' }] }]);
	});

	it('an untouched answer is named as missing, never sent and never shown sent', async () => {
		fake.questions = TWO;
		const chat = await opened();
		chat.handle({ type: 'answer', id: 'days', value: undefined });
		chat.handle({ type: 'answer', id: 'food', value: '   ' });
		chat.handle({ type: 'submit' });
		await settle();
		expect(fake.posts).toEqual([]);
		expect(group(chat.messages)).toMatchObject({ submitted: false, missing: ['days', 'food'] });
	});

	it('out of order answers map to their own question ids, and no card shows sent before submit', async () => {
		fake.questions = TWO;
		const chat = await opened();
		chat.handle({ type: 'answer', id: 'food', value: 'no nuts' });
		await settle();
		expect(group(chat.messages)).toMatchObject({ submitted: false, values: { food: 'no nuts' }, missing: ['days'] });
		chat.handle({ type: 'answer', id: 'days', value: 4 });
		chat.handle({ type: 'submit' });
		await settle();
		expect(fake.posts.map((p) => p.body)).toEqual([{ node: 'questions', answer: [{ id: 'days', answer: '4' }, { id: 'food', answer: 'no nuts' }] }]);
	});

	it('a reopened goal shows every question open again and submits all of them once', async () => {
		fake.questions = TWO;
		const first = await opened();
		first.handle({ type: 'answer', id: 'days', value: 4 });
		await settle();
		const again = await opened();
		expect(group(again.messages)).toMatchObject({ submitted: false, missing: ['days', 'food'] });
		again.handle({ type: 'answer', id: 'days', value: 3 });
		again.handle({ type: 'answer', id: 'food', value: 'none' });
		again.handle({ type: 'submit' });
		await settle();
		expect(fake.posts.map((p) => p.body)).toEqual([{ node: 'questions', answer: [{ id: 'days', answer: '3' }, { id: 'food', answer: 'none' }] }]);
	});

	it('typed composer text fills the next unanswered question and does not post', async () => {
		fake.questions = TWO;
		const chat = await opened();
		chat.handle({ type: 'answer', id: 'days', value: 4 });
		chat.handle({ type: 'send', text: 'no nuts' });
		await settle();
		expect(fake.posts).toEqual([]);
		expect(group(chat.messages)).toMatchObject({ values: { days: '4', food: 'no nuts' }, missing: [] });
	});

	it('a refused submit says so in the chat and keeps every answer', async () => {
		fake.questions = TWO;
		fake.answerStatus = 409;
		const chat = await opened();
		chat.handle({ type: 'answer', id: 'days', value: 4 });
		chat.handle({ type: 'answer', id: 'food', value: 'none' });
		chat.handle({ type: 'submit' });
		await settle();
		expect(fake.posts).toHaveLength(1);
		expect(group(chat.messages)).toMatchObject({ submitted: false, values: { days: '4', food: 'none' } });
		expect(group(chat.messages)?.error).toMatch(/not sent/i);
	});
});

describe('run limits (Q60): the owner can stop a run and retry a stopped one', () => {
	it('Stop cancels the followed run and Retry continues it', async () => {
		const chat = await opened();
		chat.handle({ type: 'stop' });
		await settle();
		chat.handle({ type: 'retry' });
		await settle();
		expect(fake.posts.map((p) => p.path)).toEqual(['/v1/runs/r1/cancel', '/v1/runs/r1/retry']);
	});
});

describe('sent goal chat messages (B62)', () => {
	it('shows a sent message at once and keeps it in the thread after the page opens again', async () => {
		const data = new Map<string, string>();
		const store = { getItem: (k: string) => data.get(k) ?? null, setItem: (k: string, v: string) => void data.set(k, v), removeItem: (k: string) => void data.delete(k) };
		const chat = new GoalChat('g2', 'Tell the agents anything.', store);
		chat.handle({ type: 'send', text: 'Move my runs to the mornings' });
		expect(texts(chat.messages)).toContain('Move my runs to the mornings');
		await settle();
		const again = new GoalChat('g2', 'Tell the agents anything.', store);
		expect(texts(again.messages)).toEqual(['Tell the agents anything.', 'Move my runs to the mornings']);
		expect(texts(new GoalChat('g3', 'Hi', store).messages)).toEqual(['Hi']);
	});
});

describe('run again at approve (B70, B71)', () => {
	const plans = (messages: ChatMessage[]) => messages.filter((m) => m.type === 'plan');

	it('b71: typing while the run waits at approve keeps one plan card, not one per message', async () => {
		fake.node = 'approve';
		const chat = await opened();
		expect(plans(chat.thread)).toHaveLength(1);
		await chat.send('Approve it please');
		await settle();
		await chat.send('Yes, approve');
		await settle();
		expect(plans(chat.thread)).toHaveLength(1);
		expect(texts(chat.thread)).toEqual(expect.arrayContaining(['Approve it please', 'Yes, approve']));
	});

	it('b70: an approved plan card stays approved and the next waiting plan card is not', async () => {
		fake.node = 'approve';
		const chat = await opened();
		expect(plans(chat.thread)).toEqual([expect.objectContaining({ approved: false })]);
		await chat.decide(true);
		await settle();
		expect(plans(chat.thread).every((m) => m.type === 'plan' && m.approved === true)).toBe(true);
	});

	it('b70: a refused approval leaves the card approvable', async () => {
		fake.node = 'approve';
		fake.answerStatus = 409;
		const chat = await opened();
		await chat.decide(true);
		await settle();
		expect(plans(chat.thread)).toEqual([expect.objectContaining({ approved: false })]);
	});

	it('b70: a goal whose runs could not be read is unreachable, not idle', async () => {
		fake.runsStatus = 500;
		const chat = new GoalChat('g1', 'Hi');
		await chat.open();
		expect(chat.unreachable).toBe(true);
		fake.runsStatus = 200;
		fake.node = 'approve';
		await chat.open();
		await settle();
		expect(chat.unreachable).toBe(false);
		expect(chat.view?.waiting[0]?.node).toBe('approve');
	});
});
