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
	private listeners = new Set<(runId: string) => void>();

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

	/** Calls `fn` with the run id of every Realtime insert, of any run. Returns the unsubscribe. */
	onAny(fn: (runId: string) => void): () => void {
		this.listeners.add(fn);
		return () => void this.listeners.delete(fn);
	}

	/** A Realtime insert for `runId`: tells any-run listeners, then wakes its waiters or keeps it for the next wait. */
	notify(runId: string) {
		this.listeners.forEach((fn) => fn(runId));
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

const BURST_MS = 2000;
const BUSY_MS = 15_000;

/**
 * `fn` at most once per `ms`: the first call runs at once, later calls in the window run once
 * when it ends, so the last of a burst of run events is never dropped.
 */
export function throttled(fn: () => void, ms: number): () => void {
	let last = -Infinity;
	let timer: ReturnType<typeof setTimeout> | null = null;
	const fire = () => {
		timer = null;
		last = Date.now();
		fn();
	};
	return () => {
		if (timer) return;
		const wait = last + ms - Date.now();
		if (wait <= 0) return fire();
		timer = setTimeout(fire, wait);
	};
}

/**
 * Keeps a runs list fresh: `reload` on any run's Realtime insert (at most every 2 s), when the
 * app comes back to the foreground, and every 15 s while `busy` (a run works, so a stop that
 * inserts no event still shows). Returns the function that stops it.
 */
export function refreshRuns(reload: () => void, busy: () => boolean): () => void {
	void connectWakeups();
	const off = wakeups.onAny(throttled(reload, BURST_MS));
	const onVisible = () => document.visibilityState === 'visible' && reload();
	document.addEventListener('visibilitychange', onVisible);
	const timer = setInterval(() => busy() && reload(), BUSY_MS);
	return () => {
		off();
		clearInterval(timer);
		document.removeEventListener('visibilitychange', onVisible);
	};
}

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
