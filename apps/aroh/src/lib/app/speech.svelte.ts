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

/** On-device voice input: `start` streams partial text, `stop` resolves the final transcript. */
export interface Speech {
	start(onPartial: (text: string) => void): Promise<void>;
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

let partials: PluginListener | null = null;

/** Drops the partial-result listener, if any. */
async function dropPartials(): Promise<void> {
	const listener = partials;
	partials = null;
	await listener?.unregister().catch(() => undefined);
}

/** The `aroh-speech` plugin: recognition runs on the iPhone and audio never leaves it. */
export const speech: Speech = {
	async start(onPartial) {
		await dropPartials();
		try {
			await invoke('plugin:aroh-speech|start');
		} catch (raw) {
			throw speechError(raw);
		}
		partials = await addPluginListener<{ text: string }>('aroh-speech', 'partial', (p) => onPartial(p.text)).catch(() => null);
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

/**
 * One mic button: `toggle` starts listening or stops and hands the final words to `ontext`, which
 * also gets each partial. `error` is the owner-facing reason the last start or stop failed.
 */
export class Dictation {
	listening = $state(false);
	error = $state<string | null>(null);

	constructor(
		private readonly voice: Speech,
		private readonly ontext: (text: string) => void
	) {}

	/** Starts or stops listening; never throws. */
	async toggle(): Promise<void> {
		this.error = null;
		try {
			if (!this.listening) {
				this.listening = true;
				await this.voice.start((text) => text.trim() && this.ontext(text));
				return;
			}
			this.listening = false;
			const text = (await this.voice.stop()).trim();
			if (text) this.ontext(text);
		} catch (e) {
			this.listening = false;
			this.error = e instanceof Error ? e.message : String(e);
		}
	}
}
