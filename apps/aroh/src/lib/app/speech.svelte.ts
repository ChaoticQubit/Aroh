import { addPluginListener, invoke, type PluginListener } from '@tauri-apps/api/core';

/** Error codes the `aroh-speech` plugin rejects with, plus `unknown` for anything else. */
export type SpeechErrorCode = 'unsupported' | 'mic_denied' | 'speech_denied' | 'on_device_unavailable' | 'recognizer_unavailable' | 'busy' | 'not_started' | 'audio_failed' | 'superseded' | 'unknown';

const CODES: readonly SpeechErrorCode[] = ['unsupported', 'mic_denied', 'speech_denied', 'on_device_unavailable', 'recognizer_unavailable', 'busy', 'not_started', 'audio_failed', 'superseded'];

/** A speech plugin failure with a stable `code` and a message fit to show the owner. */
export class SpeechError extends Error {
	readonly code: SpeechErrorCode;

	constructor(code: SpeechErrorCode, message: string) {
		super(message);
		this.code = code;
	}
}

/**
 * On-device voice input: `start` streams partial text, and `onEnded` gets the last text when the
 * recognizer finishes on its own after a pause. `stop` resolves the final transcript. Every partial,
 * ended and final text is the whole session so far, never only the latest utterance.
 */
export interface Speech {
	start(onPartial: (text: string) => void, onEnded?: (text: string) => void): Promise<void>;
	stop(): Promise<string>;
}

/** Maps a plugin rejection (`{code, message}` or a string) to a `SpeechError`. */
function speechError(raw: unknown): SpeechError {
	if (raw instanceof SpeechError) return raw;
	if (typeof raw === 'object' && raw !== null) {
		const { code, message } = raw as { code?: unknown; message?: unknown };
		return new SpeechError(CODES.find((c) => c === code) ?? 'unknown', typeof message === 'string' ? message : 'Voice input failed.');
	}
	return new SpeechError('unknown', typeof raw === 'string' ? raw : 'Voice input failed.');
}

let partials: PluginListener[] = [];

/** Drops the partial and ended listeners, if any. */
async function dropPartials(): Promise<void> {
	const listeners = partials;
	partials = [];
	await Promise.all(listeners.map((l) => l.unregister().catch(() => undefined)));
}

/** The `aroh-speech` plugin: recognition runs on the iPhone and audio never leaves it. */
export const speech: Speech = {
	async start(onPartial, onEnded) {
		await dropPartials();
		try {
			await invoke('plugin:aroh-speech|start');
		} catch (raw) {
			throw speechError(raw);
		}
		try {
			partials.push(await addPluginListener<{ text: string }>('aroh-speech', 'partial', (p) => onPartial(p.text)));
			partials.push(
				await addPluginListener<{ text: string }>('aroh-speech', 'ended', (p) => {
					void dropPartials();
					onEnded?.(p.text);
				})
			);
		} catch (raw) {
			await dropPartials();
			await invoke('plugin:aroh-speech|stop').catch(() => undefined);
			throw new SpeechError('unknown', `Voice input could not show your words: ${raw instanceof Error ? raw.message : String(raw)}`);
		}
	},
	async stop() {
		try {
			return (await invoke<{ transcript: string }>('plugin:aroh-speech|stop')).transcript;
		} catch (raw) {
			throw speechError(raw);
		} finally {
			await dropPartials();
		}
	}
};

const DENIED: ReadonlySet<SpeechErrorCode> = new Set(['mic_denied', 'speech_denied']);

const AGAIN = 'Voice input stopped. Tap the mic to try again.';

/** The owner-facing text per failure code; null means the failure needs no notice. Native messages are never shown. */
const MESSAGES: Record<SpeechErrorCode, string | null> = {
	mic_denied: 'Microphone is off for Aroh. Turn it on in Settings.',
	speech_denied: 'Speech recognition is off for Aroh. Turn it on in Settings.',
	unsupported: 'Voice input is not available on this device.',
	on_device_unavailable: 'Voice input is not available for this language on this iPhone.',
	recognizer_unavailable: 'Voice input is not available right now. Try again in a moment.',
	audio_failed: 'The microphone could not start. Try again.',
	busy: AGAIN,
	unknown: AGAIN,
	not_started: null,
	superseded: null
};

/** The failure code of `e`, `unknown` for anything that is not a `SpeechError`. */
const codeOf = (e: unknown): SpeechErrorCode => (e instanceof SpeechError ? e.code : 'unknown');

/** Opens Aroh's page in the iPhone Settings app, where microphone, speech and location access are turned on. */
export const openSettings = () => invoke<void>('plugin:aroh-speech|open_settings');

/** Joins non-empty parts with one space. */
const joined = (...parts: string[]) => parts.map((p) => p.trim()).filter(Boolean).join(' ');

let active: Dictation | null = null;
let watching = false;

/** Stops the listening mic, keeping its words, when the app goes to the background. Registers once, only where a page exists. */
function watchBackground(): void {
	if (watching || typeof document === 'undefined') return;
	watching = true;
	document.addEventListener('visibilitychange', () => {
		if (document.visibilityState === 'hidden') void active?.release();
	});
}

/**
 * One mic button: `toggle` starts listening or stops. The session text from `Speech` is appended to
 * the draft as it was when the mic started (read through `draft`) and the whole text goes to `ontext`,
 * on each partial and on the final words, so a newer partial replaces the older one in the draft.
 * The recognizer ending on its own after a pause stops listening and keeps the text. Taps run in
 * order, so a tap during a start stops once the start lands. The native session is one per app: a
 * mic starting elsewhere releases this one, and `release` (component destroyed, app in background)
 * stops it. `error` is fixed owner-facing copy for the last failure; `denied` means access is off.
 */
export class Dictation {
	listening = $state(false);
	error = $state<string | null>(null);
	denied = $state(false);
	private base = '';
	private queue: Promise<void> = Promise.resolve();

	constructor(
		private readonly voice: Speech,
		private readonly draft: () => string,
		private readonly ontext: (text: string) => void
	) {}

	/** Takes the session text so far and hands the draft plus it to `ontext`; blank text changes nothing. */
	private hear(text: string): void {
		if (!text.trim()) return;
		this.ontext(joined(this.base, text));
	}

	/** Runs `step` after every earlier tap or release has settled. */
	private enqueue(step: () => Promise<void>): Promise<void> {
		this.queue = this.queue.then(step);
		return this.queue;
	}

	/** Marks this mic off and gives up the app's one native session if it held it. */
	private off(): void {
		this.listening = false;
		if (active === this) active = null;
	}

	/** Shows the notice for failure `e`, or none when the code needs none. */
	private fail(e: unknown): void {
		this.off();
		const code = codeOf(e);
		this.denied = DENIED.has(code);
		this.error = MESSAGES[code];
	}

	/** Starts the native session, stopping a stale busy one once and retrying. */
	private async begin(): Promise<void> {
		watchBackground();
		if (active && active !== this) active.off();
		active = this;
		this.listening = true;
		this.base = this.draft();
		const start = () =>
			this.voice.start(
				(text) => this.hear(text),
				(text) => {
					this.off();
					this.hear(text);
				}
			);
		try {
			await start();
		} catch (e) {
			if (codeOf(e) !== 'busy') throw e;
			await this.voice.stop().catch(() => undefined);
			await start();
		}
		assert(this.listening === (active === this), 'a listening mic holds the native session');
	}

	/** Stops the native session and keeps its final words; a session the native side already ended is no failure. */
	private async end(): Promise<void> {
		this.off();
		try {
			this.hear(await this.voice.stop());
		} catch (e) {
			if (codeOf(e) !== 'not_started') throw e;
		}
	}

	/** Starts or stops listening; never throws. */
	toggle(): Promise<void> {
		return this.enqueue(async () => {
			this.error = null;
			this.denied = false;
			try {
				if (this.listening) await this.end();
				else await this.begin();
			} catch (e) {
				this.fail(e);
			}
		});
	}

	/** Stops listening, keeping the words, if this mic still holds the session; idempotent and never throws. */
	release(): Promise<void> {
		return this.enqueue(async () => {
			if (!this.listening) return;
			try {
				await this.end();
			} catch {
				this.off();
			}
		});
	}
}

/** Throws when internal invariant `ok` is broken. */
function assert(ok: boolean, message: string): asserts ok {
	if (!ok) throw new Error(`invariant: ${message}`);
}
