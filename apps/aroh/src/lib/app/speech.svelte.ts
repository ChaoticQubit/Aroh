import { addPluginListener, invoke, type PluginListener } from '@tauri-apps/api/core';

/** Error codes the `aroh-speech` plugin rejects with, plus `unknown` for anything else. */
export type SpeechErrorCode = 'unsupported' | 'mic_denied' | 'speech_denied' | 'on_device_unavailable' | 'recognizer_unavailable' | 'busy' | 'not_started' | 'audio_failed' | 'unknown';

const CODES: readonly SpeechErrorCode[] = ['unsupported', 'mic_denied', 'speech_denied', 'on_device_unavailable', 'recognizer_unavailable', 'busy', 'not_started', 'audio_failed'];

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

const DENIED: Partial<Record<SpeechErrorCode, string>> = {
	mic_denied: 'Microphone is off for Aroh. Turn it on in Settings.',
	speech_denied: 'Speech recognition is off for Aroh. Turn it on in Settings.'
};

/** Opens Aroh's page in the iPhone Settings app, where microphone, speech and location access are turned on. */
export const openSettings = () => invoke<void>('plugin:aroh-speech|open_settings');

/** Joins non-empty parts with one space. */
const joined = (...parts: string[]) => parts.map((p) => p.trim()).filter(Boolean).join(' ');

/**
 * One mic button: `toggle` starts listening or stops. The session text from `Speech` is appended to
 * the draft as it was when the mic started (read through `draft`) and the whole text goes to `ontext`,
 * on each partial and on the final words, so a newer partial replaces the older one in the draft.
 * The recognizer ending on its own after a pause stops listening and keeps the text.
 * `error` is the owner-facing reason the last start or stop failed; `denied` means access is off.
 */
export class Dictation {
	listening = $state(false);
	error = $state<string | null>(null);
	denied = $state(false);
	private base = '';

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

	/** Starts or stops listening; never throws. */
	async toggle(): Promise<void> {
		this.error = null;
		this.denied = false;
		try {
			if (!this.listening) {
				this.listening = true;
				this.base = this.draft();
				await this.voice.start(
					(text) => this.hear(text),
					(text) => {
						this.listening = false;
						this.hear(text);
					}
				);
				return;
			}
			this.listening = false;
			this.hear(await this.voice.stop());
		} catch (e) {
			this.listening = false;
			const code = e instanceof SpeechError ? e.code : 'unknown';
			this.denied = code in DENIED;
			this.error = DENIED[code] ?? (e instanceof Error ? e.message : String(e));
		}
	}
}
