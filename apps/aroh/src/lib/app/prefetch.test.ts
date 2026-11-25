import { describe, expect, it, vi } from 'vitest';
import { Engine, type Call } from './engine';
import { goalTarget, prefetchGoal } from './prefetch';

vi.mock('@tauri-apps/api/core', () => ({ invoke: vi.fn() }));

const RUNS = [
	{ run_id: 'r2', goal_id: 'g1', workflow: 'replan', state: 'done', created_at: '', updated_at: '' },
	{ run_id: 'r1', goal_id: 'g1', workflow: 'new-goal', state: 'done', created_at: '', updated_at: '' }
];

describe('prefetch on tap (B87)', () => {
	it('reads the goal runs, personas and both run views once, shared with the page opening at the same time', async () => {
		const sent: string[] = [];
		const call: Call = async (method, path) => {
			sent.push(`${method} ${path}`);
			await new Promise((r) => setTimeout(r, 5));
			return { status: 200, body: path === '/v1/runs?goal_id=g1' ? RUNS : path === '/v1/personas' ? [] : { run_id: path.slice(-2) } };
		};
		const e = new Engine(call);
		const tap = prefetchGoal('g1', e);
		const again = prefetchGoal('g1', e);
		const page = Promise.all([e.runs({ goalId: 'g1' }), e.personas()]);
		await Promise.all([tap, again, page]);
		expect(sent.sort()).toEqual(['GET /v1/personas', 'GET /v1/runs/r1', 'GET /v1/runs/r2', 'GET /v1/runs?goal_id=g1']);
	});

	it('finds the goal a tapped element opens, never the new-goal form', () => {
		expect(goalTarget(null, '/goals/g1')).toBe('g1');
		expect(goalTarget(null, '/goals/g1/chat')).toBe('g1');
		expect(goalTarget('g2', null)).toBe('g2');
		expect(goalTarget(null, '/goals/new')).toBeNull();
		expect(goalTarget(null, '/goals')).toBeNull();
		expect(goalTarget(null, '/inbox')).toBeNull();
		expect(goalTarget(null, null)).toBeNull();
	});
});
