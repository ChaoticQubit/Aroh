import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { Wakeups } from './realtime';

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
});
