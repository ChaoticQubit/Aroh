import { describe, expect, it } from 'vitest';
import { clearAll, deleteEntry, dismissedList, MAX_DISMISSED, runDelete, type InboxDeps } from './inbox';

/** Fake app deps that record every call; `refuse` lists run ids whose cancel is refused. */
function deps(states: Record<string, string>, refuse: string[] = []) {
	const calls: unknown[][] = [];
	const d: InboxDeps = {
		decide: async (...a) => void calls.push(['decide', ...a]),
		ignoreGroup: async (n) => void calls.push(['ignoreGroup', n]),
		ignorePlace: async (id) => void calls.push(['ignorePlace', id]),
		dismissInsights: async (ids) => void calls.push(['dismissInsights', ids]),
		storeRuns: async (ids) => void calls.push(['storeRuns', ids]),
		cancelRun: async (id) => (calls.push(['cancelRun', id]), refuse.includes(id) ? { ok: false, reason: 'error', message: 'conflict' } : { ok: true, value: null }),
		runState: (id) => states[id]
	};
	return { d, calls };
}

describe('deleting Inbox entries (B115)', () => {
	it('b115: deleting a proposal dismisses it through the Inbox decision, a group answers Ignore', async () => {
		const { d, calls } = deps({});
		await deleteEntry('approval', 'p1', d);
		await deleteEntry('suggestion', 'f1', d);
		await deleteEntry('triage', 'Work', d);
		await deleteEntry('place', 'pl1', d);
		await deleteEntry('insight', 'i1', d);
		expect(calls).toEqual([['decide', 'approval', 'p1', 'reject'], ['decide', 'suggestion', 'f1', 'reject'], ['ignoreGroup', 'Work'], ['ignorePlace', 'pl1'], ['dismissInsights', ['i1']]]);
	});

	it('b115: a waiting run is cancelled, then stored as dismissed; a failed one is only stored', async () => {
		const { d, calls } = deps({ r1: 'waiting_human', r2: 'failed' });
		await deleteEntry('run', 'r1', d);
		await deleteEntry('run', 'r2', d);
		expect(calls).toEqual([['cancelRun', 'r1'], ['storeRuns', ['r1']], ['storeRuns', ['r2']]]);
	});

	it('b115: a refused cancel is not stored and says why', async () => {
		const { d, calls } = deps({ r1: 'running' }, ['r1']);
		expect(await deleteEntry('run', 'r1', d)).toEqual(['conflict']);
		expect(calls).toEqual([['cancelRun', 'r1']]);
	});

	it('b115: Clear all deletes every kind, ignores every open calendar group and writes insights and runs once', async () => {
		const { d, calls } = deps({ r1: 'waiting_human', r2: 'failed' });
		await clearAll({ places: ['pl1'], approvals: ['p1', 'pattern:x'], suggestions: ['f1'], insights: ['i1', 'i2'], groups: ['Work', 'Gym'], runs: ['r1', 'r2'] }, d);
		expect(calls).toEqual([
			['ignorePlace', 'pl1'],
			['decide', 'approval', 'p1', 'reject'],
			['decide', 'approval', 'pattern:x', 'reject'],
			['decide', 'suggestion', 'f1', 'reject'],
			['dismissInsights', ['i1', 'i2']],
			['ignoreGroup', 'Work'],
			['ignoreGroup', 'Gym'],
			['cancelRun', 'r1'],
			['storeRuns', ['r1', 'r2']]
		]);
	});
});

const id = (i: number) => `0190f2a0-0000-7000-8000-${String(i).padStart(12, '0')}`;

describe('dismissed lists (B115)', () => {
	it('b115: a 31st dismissal keeps the first one hidden', () => {
		const ids = Array.from({ length: 31 }, (_, i) => id(i));
		let list = '';
		for (const x of ids) list = dismissedList(list, [x], new Set(ids));
		expect(list.split(',')).toContain(id(0));
		expect(list.split(',')).toHaveLength(31);
	});

	it('b115: drops ids whose item is gone, keeps all when the live items are unknown', () => {
		expect(dismissedList(`${id(1)},${id(2)}`, [id(3)], new Set([id(2), id(3)]))).toBe(`${id(2)},${id(3)}`);
		expect(dismissedList(`${id(1)},${id(2)}`, [id(3)], null)).toBe(`${id(1)},${id(2)},${id(3)}`);
	});

	it('b115: adds each id once and never passes the stored bound', () => {
		expect(dismissedList(id(1), [id(1), id(2)], null)).toBe(`${id(1)},${id(2)}`);
		const many = Array.from({ length: MAX_DISMISSED + 5 }, (_, i) => id(i));
		const out = dismissedList(many.join(','), [id(9999)], null).split(',');
		expect(out).toHaveLength(MAX_DISMISSED);
		expect(out.at(-1)).toBe(id(9999));
	});
});

describe('deleting a run (B115)', () => {
	it('b115: a run that waits for the owner, is paused or still works is cancelled after a confirm; a stopped one is hidden', () => {
		for (const st of ['waiting_human', 'waiting_budget', 'queued', 'running', 'checkpointed']) expect(runDelete(st)).toBe('cancel');
		expect(runDelete('failed')).toBe('hide');
	});
});
