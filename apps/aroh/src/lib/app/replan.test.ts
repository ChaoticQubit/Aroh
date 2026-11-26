import { describe, expect, it, vi } from 'vitest';
import { movedLines, NOT_STARTED_MS, phaseOf, type EditReplan, type Observed } from './replan';
import type { RunView } from './engine';

vi.mock('@tauri-apps/api/core', () => ({ invoke: vi.fn() }));

const SAVED = 1_000_000;
const ZONE = 'UTC';

function obs(over: Partial<Observed>): Observed {
	return { savedAt: SAVED, now: SAVED + 5_000, server: null, run: null, runSeenAt: null, proposals: null, zone: ZONE, ...over };
}

const ok = (v: Partial<EditReplan>) => ({ ok: true as const, value: { event_at: null, checked: false, run_id: null, waiting_on: null, ...v } });

function run(status: RunView['status'], steps: string[] = []): RunView {
	const events = steps.map((step, i) => ({ seq: i, at_ms: i, body: { type: 'step_started', step } }));
	return { run_id: 'r1', goal_id: 'g1', workflow: 'replan', status, waiting: [], events };
}

describe('phaseOf', () => {
	it('says Saved while the server has not checked the edit yet', () => {
		expect(phaseOf(obs({ server: ok({ event_at: '2026-11-24T10:00:00Z' }) })).kind).toBe('saved');
		expect(phaseOf(obs({})).kind).toBe('saved');
	});

	it('never stays silent past a minute or when the server is out of reach', () => {
		const late = SAVED + NOT_STARTED_MS;
		const noEvent = phaseOf(obs({ now: late, server: ok({}) }));
		expect(noEvent).toMatchObject({ kind: 'not_started' });
		expect(noEvent.text).toContain('has not reached the server');
		const unchecked = phaseOf(obs({ now: late, server: ok({ event_at: '2026-11-24T10:00:00Z' }) }));
		expect(unchecked.text).toContain('has not checked it');
		const offline = phaseOf(obs({ server: { ok: false, reason: 'error', message: 'connection refused' } }));
		expect(offline).toMatchObject({ kind: 'not_started' });
		expect(offline.text).toContain('could not reach the server');
		const signedOut = phaseOf(obs({ server: { ok: false, reason: 'needs_engine', message: 'sign in first' } }));
		expect(signedOut.text).toContain('Sign in');
	});

	it('waits behind an earlier replan without calling it silence', () => {
		const p = phaseOf(obs({ now: SAVED + NOT_STARTED_MS * 2, server: ok({ event_at: 'x', waiting_on: 'r0' }) }));
		expect(p).toMatchObject({ kind: 'replanning' });
		expect(p.text).toContain('earlier replan');
	});

	it('ends at Saved when nothing clashed', () => {
		expect(phaseOf(obs({ server: ok({ event_at: 'x', checked: true }) }))).toMatchObject({ kind: 'clear' });
	});

	it('shows the step and elapsed time while the run moves', () => {
		const p = phaseOf(obs({ now: SAVED + 75_000, server: ok({ event_at: 'x', checked: true, run_id: 'r1' }), run: run({ status: 'running' }, ['adapt']), runSeenAt: SAVED + 60_000 }));
		expect(p).toMatchObject({ kind: 'replanning' });
		expect(p.text).toBe('Replanning: Looking at what changed · 1:15');
	});

	it('flags a run the engine never picks up', () => {
		const queued = obs({ server: ok({ event_at: 'x', checked: true, run_id: 'r1' }), run: run({ status: 'queued' }), runSeenAt: SAVED });
		expect(phaseOf({ ...queued, now: SAVED + 30_000 }).kind).toBe('replanning');
		const stuck = phaseOf({ ...queued, now: SAVED + NOT_STARTED_MS });
		expect(stuck).toMatchObject({ kind: 'not_started' });
		expect(stuck.text).toContain('has not picked it up');
	});

	it('maps waiting, paused and failed runs', () => {
		const base = obs({ server: ok({ event_at: 'x', checked: true, run_id: 'r1' }), runSeenAt: SAVED });
		expect(phaseOf({ ...base, run: run({ status: 'waiting_human' }) })).toMatchObject({ kind: 'needs_you', text: 'Replanning needs your OK' });
		const failed = phaseOf({ ...base, run: run({ status: 'failed', error: 'model timeout' }) });
		expect(failed).toMatchObject({ kind: 'failed', runId: 'r1' });
		expect(failed.text).toContain('model timeout');
		expect(phaseOf({ ...base, run: run({ status: 'waiting_budget', reason: { kind: 'budget', limit: 'monthly' } }) }).kind).toBe('failed');
	});

	it('names what moved once the run is done, with Undo only for applied changes', () => {
		const base = obs({ server: ok({ event_at: 'x', checked: true, run_id: 'r1' }), runSeenAt: SAVED, run: run({ status: 'done' }) });
		expect(phaseOf(base).kind).toBe('replanning');
		const applied = {
			id: 'p1', goal_id: 'g1', summary: 's', size: 'small' as const, status: 'applied', created_at: 'x',
			diff: { changes: [{ op: 'move', title: 'Read', after: { start_at: '2026-11-24T15:00:00Z', end_at: '2026-11-24T15:30:00Z' } }] }
		};
		const done = phaseOf({ ...base, proposals: [applied] });
		expect(done).toMatchObject({ kind: 'updated', runId: 'r1', undo: true });
		expect(done.text).toBe('Updated: moved Read to Tue 15:00.');
		expect(phaseOf({ ...base, proposals: [] })).toMatchObject({ kind: 'updated', undo: false, text: 'Updated: nothing else needed to move.' });
		expect(phaseOf({ ...base, proposals: [{ ...applied, status: 'undone' }] })).toMatchObject({ kind: 'undone' });
	});
});

describe('movedLines', () => {
	it('words moves, removals and additions and skips bad rows', () => {
		const lines = movedLines(
			{ changes: [
				{ op: 'move', title: 'Read', after: { start_at: '2026-11-24T15:00:00Z' } },
				{ op: 'delete', title: 'Walk' },
				{ op: 'create', title: 'Plank', after: { start_at: '2026-11-25T07:00:00Z' } },
				{ op: 'move' },
				'junk'
			] },
			ZONE
		);
		expect(lines).toEqual(['moved Read to Tue 15:00', 'removed Walk', 'added Plank on Wed 07:00']);
		expect(movedLines(null, ZONE)).toEqual([]);
	});
});
