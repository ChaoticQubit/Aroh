import { describe, expect, it, vi } from 'vitest';
import { cachedPath, EngineCache, MAX_BYTES, MAX_ENTRIES, type CacheStore } from './cache';
import { Engine, type Answer, type Call } from './engine';

vi.mock('@tauri-apps/api/core', () => ({ invoke: vi.fn() }));

/** A web storage stand-in over a map; `broken` makes every access throw, as a blocked store does. */
function store(broken = false): CacheStore & { map: Map<string, string> } {
	const map = new Map<string, string>();
	const guard = () => {
		if (broken) throw new Error('SecurityError: storage is blocked');
	};
	return {
		map,
		getItem: (k) => (guard(), map.get(k) ?? null),
		setItem: (k, v) => (guard(), void map.set(k, v)),
		removeItem: (k) => (guard(), void map.delete(k))
	};
}

/** An engine over a fake transport answering `answer(method, path)`, with every request logged in `sent`. */
function engineWith(cache: EngineCache, answer: (method: string, path: string) => Answer) {
	const sent: string[] = [];
	const call: Call = async (method, path) => (sent.push(`${method} ${path}`), answer(method, path));
	return { e: new Engine(call, cache), sent };
}

const RUN = { run_id: 'r1', goal_id: 'g1', workflow: 'new-goal', state: 'done', created_at: '', updated_at: '' };

describe('engine read cache (B87)', () => {
	it('serves the last read at once, then the fresh one once it lands', async () => {
		const cache = new EngineCache(store());
		cache.owner('u1');
		let state = 'done';
		const { e } = engineWith(cache, () => ({ status: 200, body: [{ ...RUN, state }] }));
		expect(e.cachedRuns({ goalId: 'g1' })).toBeUndefined();
		await e.runs({ goalId: 'g1' });
		expect(e.cachedRuns({ goalId: 'g1' })?.[0].state).toBe('done');
		state = 'running';
		const fresh = await e.runs({ goalId: 'g1' });
		expect(fresh.ok && fresh.value[0].state).toBe('running');
		expect(e.cachedRuns({ goalId: 'g1' })?.[0].state).toBe('running');
	});

	it('survives an app restart for the same account and is empty for another', async () => {
		const disk = store();
		const first = new EngineCache(disk);
		first.owner('u1');
		const { e } = engineWith(first, () => ({ status: 200, body: { minutes: 9 } }));
		await e.runLimit();
		const again = new EngineCache(disk);
		expect(again.get('/v1/settings/run-limit'), 'nothing before the account is confirmed').toBeUndefined();
		again.owner('u1');
		expect(again.get('/v1/settings/run-limit')).toEqual({ minutes: 9 });
		again.owner('u2');
		expect(again.get('/v1/settings/run-limit')).toBeUndefined();
		again.owner('u1');
		expect(again.get('/v1/settings/run-limit'), 'an account switch cleared it').toBeUndefined();
	});

	it('clears everything on sign-out, and a read of the old account landing late is dropped', async () => {
		const disk = store();
		const cache = new EngineCache(disk);
		cache.owner('u1');
		let release: () => void = () => {};
		const gate = new Promise<void>((r) => (release = r));
		const e = new Engine(async (_m, path) => {
			if (path === '/v1/insights') await gate;
			return { status: 200, body: path === '/v1/insights' ? [] : { minutes: 5 } };
		}, cache);
		await e.runLimit();
		const late = e.insights();
		cache.owner(null);
		expect(cache.get('/v1/settings/run-limit')).toBeUndefined();
		cache.owner('u1');
		release();
		await late;
		expect(cache.get('/v1/insights')).toBeUndefined();
		expect([...disk.map.values()].join('')).not.toContain('minutes');
	});

	it('never stores keys or anything key-related, and only allowlisted reads', async () => {
		const disk = store();
		const cache = new EngineCache(disk);
		cache.owner('u1');
		const { e } = engineWith(cache, (_m, path) => ({ status: 200, body: path.includes('keys') ? [{ setting_id: 's', vendor: 'anthropic', last4: 'SECRET4' }] : { ok: true, error: null } }));
		await e.keys();
		await e.testConnection('s');
		expect(cache.get('/v1/keys')).toBeUndefined();
		expect([...disk.map.values()].join('')).not.toContain('SECRET4');
		expect(cachedPath('/v1/keys')).toBe(false);
		expect(cachedPath('/v1/keys/s/test')).toBe(false);
		expect(cachedPath('/v1/runs/r1?after_seq=3')).toBe(false);
		expect(cachedPath('/v1/runs?goal_id=g1&state=waiting_human')).toBe(true);
		expect(cachedPath('/v1/runs/r1')).toBe(true);
		expect(cachedPath('/v1/account')).toBe(true);
	});

	it('keeps only the consent of the account read, and a failed read changes nothing', async () => {
		const cache = new EngineCache(store());
		cache.owner('u1');
		let status = 200;
		const { e } = engineWith(cache, () => ({ status, body: { ai_consent_at: '2026-11-01T00:00:00Z', email: 'owner@example.com' } }));
		await e.consent();
		expect(cache.get('/v1/account')).toEqual({ ai_consent_at: '2026-11-01T00:00:00Z' });
		expect(e.cachedConsent()).toBe(true);
		status = 500;
		await e.consent();
		expect(e.cachedConsent()).toBe(true);
	});

	it('a write drops the reads it changes', async () => {
		const cache = new EngineCache(store());
		cache.owner('u1');
		const { e } = engineWith(cache, (method, path) => ({ status: method === 'POST' ? 202 : 200, body: path.startsWith('/v1/personas') ? [] : path.startsWith('/v1/runs/') ? { run_id: 'r1' } : [RUN] }));
		await Promise.all([e.runs({ goalId: 'g1' }), e.run('r1'), e.personas(), e.insights(), e.runLimit()]);
		await e.replan('g1', 'Busy week');
		expect([e.cachedRuns({ goalId: 'g1' }), e.cachedRun('r1'), e.cachedInsights()]).toEqual([undefined, undefined, undefined]);
		expect(e.cachedPersonas()).toEqual([]);
		await e.editPersona('coach', { display_name: 'C', role: 'r', instructions: 'i', guardrails: [] } as never);
		expect(e.cachedPersonas()).toBeUndefined();
		expect(e.cachedRunLimit()).toBeDefined();
		await e.setRunLimit(9);
		expect(e.cachedRunLimit()).toBeUndefined();
	});

	it('caps the entries and the bytes, dropping the oldest first', () => {
		const cache = new EngineCache(store());
		cache.owner('u1');
		for (let i = 0; i < MAX_ENTRIES + 5; i++) cache.put(cache.current(), `/v1/runs/r${i}`, { run_id: `r${i}` });
		expect(cache.get('/v1/runs/r0')).toBeUndefined();
		expect(cache.get(`/v1/runs/r${MAX_ENTRIES + 4}`)).toEqual({ run_id: `r${MAX_ENTRIES + 4}` });
		expect(cache.size().entries).toBe(MAX_ENTRIES);
		const big = 'x'.repeat(Math.floor(MAX_BYTES / 3));
		cache.put(cache.current(), '/v1/runs/a', { big });
		cache.put(cache.current(), '/v1/runs/b', { big });
		cache.put(cache.current(), '/v1/runs/c', { big });
		expect(cache.size().bytes).toBeLessThanOrEqual(MAX_BYTES);
		expect(cache.get('/v1/runs/c')).toEqual({ big });
		cache.put(cache.current(), '/v1/runs/huge', { big: 'x'.repeat(MAX_BYTES + 1) });
		expect(cache.get('/v1/runs/huge')).toBeUndefined();
	});

	it('works without storage, and with a storage that throws', async () => {
		for (const disk of [null, store(true)]) {
			const cache = new EngineCache(disk);
			cache.owner('u1');
			const { e } = engineWith(cache, () => ({ status: 200, body: { minutes: 7 } }));
			await e.runLimit();
			expect(e.cachedRunLimit()).toEqual({ minutes: 7 });
			cache.owner(null);
			expect(e.cachedRunLimit()).toBeUndefined();
		}
	});
});
