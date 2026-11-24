import { invoke } from '@tauri-apps/api/core';
import { listen } from '@tauri-apps/api/event';

/** Why a wait ended: a Realtime event for the run, the timer, or the socket dropping. */
export type Woken = 'event' | 'poll' | 'down';

type Waiter = { runId: string; done: (why: Woken) => void; timer: ReturnType<typeof setTimeout> };

const PENDING_MAX = 64;

/**
 * Waits between run reads. With the Realtime socket down every wait lasts `downMs` (polling).
 * With it up a wait ends on the run's insert event, or after `upMs` as a safety net. An event
 * that arrives before its wait is kept, so a read never misses it.
 */
export class Wakeups {
	private up = false;
	private pending = new Set<string>();
	private waiters: Waiter[] = [];

	constructor(
		private downMs: number,
		private upMs: number
	) {}

	/** Resolves when the next read of `runId` is due, with the reason. */
	wait(runId: string): Promise<Woken> {
		if (this.up && this.pending.delete(runId)) return Promise.resolve('event');
		return new Promise((resolve) => {
			const waiter: Waiter = {
				runId,
				done: (why) => {
					clearTimeout(waiter.timer);
					this.waiters = this.waiters.filter((w) => w !== waiter);
					resolve(why);
				},
				timer: setTimeout(() => waiter.done('poll'), this.up ? this.upMs : this.downMs)
			};
			this.waiters.push(waiter);
		});
	}

	/** A Realtime insert for `runId`: wakes its waiters, or keeps it for the next wait. */
	notify(runId: string) {
		const woken = this.waiters.filter((w) => w.runId === runId);
		if (woken.length) return woken.forEach((w) => w.done('event'));
		if (this.pending.size >= PENDING_MAX) this.pending.clear();
		this.pending.add(runId);
	}

	/** The socket joined or dropped. A drop ends every wait so polling takes over at once. */
	setUp(up: boolean) {
		this.up = up;
		if (!up) {
			this.pending.clear();
			[...this.waiters].forEach((w) => w.done('down'));
		}
	}
}

/** The app's wake-ups: 1.5 s polling while the socket is down, at most 15 s while it is up. */
export const wakeups = new Wakeups(1500, 15000);

let connected: Promise<void> | null = null;

/**
 * Feeds `wakeups` from the Rust Realtime task (`aroh://run`, `aroh://realtime`) and reads
 * whether the socket is up now. Runs once; later calls return the same promise. Outside the
 * app shell it leaves polling on.
 */
export function connectWakeups(): Promise<void> {
	connected ??= (async () => {
		try {
			await listen<{ run_id: string }>('aroh://run', (e) => wakeups.notify(e.payload.run_id));
			await listen<{ up: boolean }>('aroh://realtime', (e) => wakeups.setUp(e.payload.up));
			wakeups.setUp(await invoke<boolean>('app_realtime_up'));
		} catch {
			wakeups.setUp(false);
		}
	})();
	return connected;
}
