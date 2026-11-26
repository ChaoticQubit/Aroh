import { beforeEach, describe, expect, it, vi } from 'vitest';
import type { ChatMessage } from '$lib/ui/contract';

const fake = vi.hoisted(() => ({ calls: [] as { method: string; path: string; body: unknown }[], decided: [] as [string, boolean][] }));

vi.mock('@tauri-apps/api/core', () => ({
	invoke: vi.fn(async (_cmd: string, args: { method: string; path: string; body: unknown }) => {
		fake.calls.push(args);
		if (args.method === 'GET' && args.path === '/v1/runs?goal_id=g1&state=waiting_human') return { status: 200, body: [{ run_id: 'r1' }] };
		if (args.method === 'GET' && args.path === '/v1/runs/r1')
			return { status: 200, body: { run_id: 'r1', goal_id: 'g1', workflow: 'replan', status: { status: 'waiting_human' }, waiting: [{ node: 'approve', ask: 'approval', prompt: null, data: { proposal_record: { proposal_id: 'p1', summary: 'S', changes: [] } } }], events: [] } };
		if (args.method === 'POST' && args.path === '/v1/runs/r1/answers') return { status: 202, body: { run_id: 'r1' } };
		return { status: 200, body: [] };
	})
}));
vi.mock('./client', () => ({ decideProposal: vi.fn(async (id: string, yes: boolean) => void fake.decided.push([id, yes])) }));

const { decideProposal, withOpenProposals } = await import('./proposals');

beforeEach(() => {
	fake.calls = [];
	fake.decided = [];
});

describe('decideProposal, the one action Inbox and goal chat share (B114)', () => {
	it('b114: answers the replan run that waits on the proposal, so the engine applies it once', async () => {
		const r = await decideProposal('p1', 'g1', true);
		expect(r).toEqual({ ok: true, value: 'r1' });
		expect(fake.calls.at(-1)).toMatchObject({ method: 'POST', path: '/v1/runs/r1/answers', body: { node: 'approve', answer: true } });
		expect(fake.decided).toEqual([]);
	});

	it('b114: decides a proposal no run waits on, on the device', async () => {
		const r = await decideProposal('p2', 'g1', false);
		expect(r).toEqual({ ok: true, value: null });
		expect(fake.decided).toEqual([['p2', false]]);
	});
});

describe('withOpenProposals (B114)', () => {
	const card = (id: string, decided?: 'apply' | 'reject'): ChatMessage => ({ type: 'proposal', key: `r:${id}`, id, spec: { kind: 'proposal_card', persona: 'A', summary: id, changes: [] }, ...(decided ? { decided } : {}) });

	const ids = (messages: ChatMessage[]) => messages.map((m) => (m.type === 'proposal' ? m.id : ''));

	it('b114: hides a proposal once it leaves the open proposals, and keeps a card decided here', () => {
		const seen = new Set<string>();
		const thread = [card('p1'), card('p2'), card('p3', 'apply')];
		expect(ids(withOpenProposals(thread, [{ id: 'p1' }, { id: 'p2' }, { id: 'p3' }], seen))).toEqual(['p1', 'p2', 'p3']);
		expect(ids(withOpenProposals(thread, [{ id: 'p1' }], seen))).toEqual(['p1', 'p3']);
	});

	it('b114: keeps a card whose proposal has not reached the snapshot yet, or before it loads', () => {
		const thread = [card('p1')];
		expect(withOpenProposals(thread, [], new Set())).toEqual(thread);
		expect(withOpenProposals(thread, undefined, new Set())).toEqual(thread);
	});
});
