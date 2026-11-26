import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { Saver } from './saver';

beforeEach(() => vi.useFakeTimers());
afterEach(() => vi.useRealTimers());

describe('page saver', () => {
	it('saves once after the edits pause, with the latest content', async () => {
		const saved: string[] = [];
		const saver = new Saver<string>(800, async (v) => void saved.push(v));
		saver.schedule(() => 'a');
		await vi.advanceTimersByTimeAsync(500);
		saver.schedule(() => 'ab');
		await vi.advanceTimersByTimeAsync(799);
		expect(saved).toEqual([]);
		await vi.advanceTimersByTimeAsync(1);
		expect(saved).toEqual(['ab']);
	});

	it('saves at once on flush, and not again when the timer would have fired', async () => {
		const saved: string[] = [];
		const saver = new Saver<string>(800, async (v) => void saved.push(v));
		saver.schedule(() => 'x');
		await saver.flush();
		await vi.advanceTimersByTimeAsync(2000);
		expect(saved).toEqual(['x']);
		await saver.flush();
		expect(saved).toEqual(['x']);
	});

	it('runs saves one after another so an older one never lands last', async () => {
		const order: string[] = [];
		let release!: () => void;
		const saver = new Saver<string>(800, (v) => {
			order.push(`start ${v}`);
			return v === 'one' ? new Promise<void>((r) => (release = () => (order.push('end one'), r()))) : (order.push(`end ${v}`), Promise.resolve());
		});
		saver.schedule(() => 'one');
		const first = saver.flush();
		saver.schedule(() => 'two');
		const second = saver.flush();
		await Promise.resolve();
		expect(order).toEqual(['start one']);
		release();
		await Promise.all([first, second]);
		expect(order).toEqual(['start one', 'end one', 'start two', 'end two']);
	});

	it('reports a failed save and keeps the edit pending for the next flush', async () => {
		const errors: unknown[] = [];
		let fail = true;
		const saved: string[] = [];
		const saver = new Saver<string>(800, async (v) => {
			if (fail) throw new Error('offline');
			saved.push(v);
		}, (e) => errors.push(e));
		saver.schedule(() => 'k');
		await saver.flush();
		expect(errors).toHaveLength(1);
		fail = false;
		await saver.flush();
		expect(saved).toEqual(['k']);
	});
});
