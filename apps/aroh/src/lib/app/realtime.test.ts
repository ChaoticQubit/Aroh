import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { refreshRuns, throttled, Wakeups, wakeups } from './realtime';

vi.mock('@tauri-apps/api/event', () => ({ listen: vi.fn(async () => () => {}) }));
vi.mock('@tauri-apps/api/core', () => ({ invoke: vi.fn(async () => false) }));

describe('run wake-ups', () => {
	beforeEach(() => vi.useFakeTimers());
	afterEach(() => vi.useRealTimers());

	it('polls every interval while the socket is down', async () => {
		const w = new Wakeups(1500, 15000);
		const done = vi.fn();
		void w.wait('r1').then(done);
		await vi.advanceTimersByTimeAsync(1499);
		expect(done).not.toHaveBeenCalled();
		await vi.advanceTimersByTimeAsync(1);
		expect(done).toHaveBeenCalledWith('poll');
	});

	it('wakes on the run event while the socket is up, else waits the long interval', async () => {
		const w = new Wakeups(1500, 15000);
		w.setUp(true);
		const done = vi.fn();
		void w.wait('r1').then(done);
		await vi.advanceTimersByTimeAsync(5000);
		w.notify('other');
		await vi.advanceTimersByTimeAsync(0);
		expect(done).not.toHaveBeenCalled();
		w.notify('r1');
		await vi.advanceTimersByTimeAsync(0);
		expect(done).toHaveBeenCalledWith('event');
		const late = vi.fn();
		void w.wait('r1').then(late);
		await vi.advanceTimersByTimeAsync(15000);
		expect(late).toHaveBeenCalledWith('poll');
	});

	it('keeps an event that came before the wait, and falls back when the socket drops', async () => {
		const w = new Wakeups(1500, 15000);
		w.setUp(true);
		w.notify('r1');
		await expect(w.wait('r1')).resolves.toBe('event');
		const done = vi.fn();
		void w.wait('r1').then(done);
		w.setUp(false);
		await vi.advanceTimersByTimeAsync(0);
		expect(done).toHaveBeenCalledWith('down');
	});

	it('b97: tells every any-run listener about each insert, waiter or not, until it unsubscribes', async () => {
		const w = new Wakeups(1500, 15000);
		w.setUp(true);
		const heard = vi.fn();
		const off = w.onAny(heard);
		void w.wait('r1');
		w.notify('r1');
		w.notify('r2');
		expect(heard.mock.calls).toEqual([['r1'], ['r2']]);
		off();
		w.notify('r3');
		expect(heard).toHaveBeenCalledTimes(2);
	});
});

describe('run list refresh (b97)', () => {
	beforeEach(() => vi.useFakeTimers());
	afterEach(() => {
		vi.useRealTimers();
		vi.unstubAllGlobals();
	});

	it('throttles a burst to one call now and one at the end, so the last event is never dropped', async () => {
		const fn = vi.fn();
		const t = throttled(fn, 2000);
		t();
		t();
		t();
		expect(fn).toHaveBeenCalledTimes(1);
		await vi.advanceTimersByTimeAsync(1999);
		expect(fn).toHaveBeenCalledTimes(1);
		await vi.advanceTimersByTimeAsync(1);
		expect(fn).toHaveBeenCalledTimes(2);
		await vi.advanceTimersByTimeAsync(5000);
		expect(fn).toHaveBeenCalledTimes(2);
	});

	it('reloads on run events, on return to the app, and every 15 s only while something works', async () => {
		const listeners = new Map<string, () => void>();
		const doc = { visibilityState: 'visible', addEventListener: (k: string, f: () => void) => listeners.set(k, f), removeEventListener: (k: string) => listeners.delete(k) };
		vi.stubGlobal('document', doc);
		const reload = vi.fn();
		let busy = false;
		const stop = refreshRuns(reload, () => busy);
		await vi.advanceTimersByTimeAsync(15_000);
		expect(reload).not.toHaveBeenCalled();
		busy = true;
		await vi.advanceTimersByTimeAsync(15_000);
		expect(reload).toHaveBeenCalledTimes(1);
		busy = false;
		wakeups.notify('r1');
		expect(reload).toHaveBeenCalledTimes(2);
		listeners.get('visibilitychange')?.();
		expect(reload).toHaveBeenCalledTimes(3);
		stop();
		expect(listeners.size).toBe(0);
		wakeups.notify('r2');
		busy = true;
		await vi.advanceTimersByTimeAsync(30_000);
		expect(reload).toHaveBeenCalledTimes(3);
	});
});
