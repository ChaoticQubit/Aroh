import { listen } from '@tauri-apps/api/event';
import { formatYmd } from '$lib/ui/format';
import * as client from './client';

const RANGE_DAYS = 45;
const DAY_MS = 86_400_000;
const DAY_CHECK_MS = 60_000;

/** Today's local date in the device zone. */
function localToday(): string {
	return formatYmd(new Date().toISOString(), Intl.DateTimeFormat().resolvedOptions().timeZone);
}

/**
 * The app's loaded snapshot: reloads on `aroh://changed`, and when the local date changes it
 * runs the start-up tick so the recurrence window and missed marks move with the day.
 */
class AppData {
	snapshot = $state<client.Snapshot | null>(null);
	error = $state<string | null>(null);
	day = $state(localToday());

	/** Reads the snapshot for 45 days either side of now. */
	async load() {
		const now = Date.now();
		try {
			this.snapshot = await client.snapshot(new Date(now - RANGE_DAYS * DAY_MS).toISOString(), new Date(now + RANGE_DAYS * DAY_MS).toISOString());
			this.error = null;
		} catch (e) {
			this.error = String(e);
		}
	}

	/** Runs one write and shows its error, if any; the change event reloads the snapshot. */
	async run(write: () => Promise<unknown>) {
		try {
			await write();
			this.error = null;
		} catch (e) {
			this.error = String(e);
		}
	}

	/** Ticks and reloads when the local date moved since the last check. */
	async checkDay() {
		const today = localToday();
		if (today === this.day) return;
		this.day = today;
		await this.run(client.tick);
		await this.load();
	}

	/** Loads once and starts listening; returns the function that stops listening. */
	start(): () => void {
		void this.load();
		const unlisten = listen('aroh://changed', () => void this.load());
		const timer = setInterval(() => void this.checkDay(), DAY_CHECK_MS);
		const onVisible = () => document.visibilityState === 'visible' && void this.checkDay();
		document.addEventListener('visibilitychange', onVisible);
		return () => {
			clearInterval(timer);
			document.removeEventListener('visibilitychange', onVisible);
			void unlisten.then((stop) => stop());
		};
	}
}

/** The one loaded snapshot every route reads. */
export const data = new AppData();
