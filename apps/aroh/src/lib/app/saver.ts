/**
 * A debounced, serialized saver: `schedule` marks the content dirty and saves `delay` ms after
 * the last call; `flush` saves now. Saves run one at a time in call order, each reading the
 * content its reader returns when it starts. A failed save goes to `onerror` and stays pending.
 */
export class Saver<T> {
	private timer: ReturnType<typeof setTimeout> | undefined;
	private read: (() => T) | null = null;
	private chain: Promise<void> = Promise.resolve();

	constructor(
		private readonly delay: number,
		private readonly save: (value: T) => Promise<unknown>,
		private readonly onerror: (error: unknown) => void = () => {}
	) {
		console.assert(delay > 0, 'save delay is positive');
	}

	/** True while an edit waits to be saved. */
	get dirty(): boolean {
		return this.read !== null;
	}

	/** Marks the content returned by `read` as unsaved and restarts the pause timer. */
	schedule(read: () => T) {
		this.read = read;
		clearTimeout(this.timer);
		this.timer = setTimeout(() => void this.flush(), this.delay);
	}

	/** Saves the pending content now, after any save already running; resolves when done. */
	flush(): Promise<void> {
		clearTimeout(this.timer);
		this.timer = undefined;
		const read = this.read;
		this.read = null;
		if (!read) return this.chain;
		this.chain = this.chain.then(async () => {
			try {
				await this.save(read());
			} catch (e) {
				this.read ??= read;
				this.onerror(e);
			}
		});
		return this.chain;
	}
}
