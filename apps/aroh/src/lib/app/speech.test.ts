import { beforeEach, describe, expect, it, vi } from 'vitest';

vi.mock('@tauri-apps/api/core', () => ({ invoke: vi.fn(), addPluginListener: vi.fn() }));

const page = Object.assign(new EventTarget(), { visibilityState: 'visible' });
vi.stubGlobal('document', page);

import { Dictation, SpeechError, type Speech } from './speech.svelte';

/** A speech stand-in: `start` streams `partials`, `stop` returns `final`, or either rejects with `fail`. */
function fakeSpeech(partials: string[], final: string, fail?: { at: 'start' | 'stop'; code: SpeechError['code'] }): Speech {
	return {
		async start(onPartial) {
			if (fail?.at === 'start') throw new SpeechError(fail.code, 'Microphone access is off for Aroh. Turn it on in Settings.');
			partials.forEach(onPartial);
		},
		async stop() {
			if (fail?.at === 'stop') throw new SpeechError(fail.code, 'Nothing was heard.');
			return final;
		}
	};
}

describe('chat voice input', () => {
	it('streams what the owner says into the message and keeps the final words on stop', async () => {
		const seen: string[] = [];
		const d = new Dictation(fakeSpeech(['I read', 'I read two'], 'I read two chapters'), () => '', (text) => seen.push(text));
		await d.toggle();
		expect(d.listening).toBe(true);
		expect(seen).toEqual(['I read', 'I read two']);
		await d.toggle();
		expect(d.listening).toBe(false);
		expect(seen.at(-1)).toBe('I read two chapters');
		expect(d.error).toBeNull();
	});

	it('a denied microphone says how to turn it on, offers Settings, and is not left listening', async () => {
		const d = new Dictation(fakeSpeech([], '', { at: 'start', code: 'mic_denied' }), () => '', () => {});
		await d.toggle();
		expect(d.listening).toBe(false);
		expect(d.error).toBe('Microphone is off for Aroh. Turn it on in Settings.');
		expect(d.denied).toBe(true);
	});

	it('denied speech recognition offers Settings too', async () => {
		const d = new Dictation(fakeSpeech([], '', { at: 'start', code: 'speech_denied' }), () => '', () => {});
		await d.toggle();
		expect(d.error).toBe('Speech recognition is off for Aroh. Turn it on in Settings.');
		expect(d.denied).toBe(true);
	});

	it('any other failure shows fixed owner copy without the Settings button, never the native message', async () => {
		const d = new Dictation(fakeSpeech([], '', { at: 'stop', code: 'audio_failed' }), () => '', () => {});
		await d.toggle();
		await d.toggle();
		expect(d.error).toBe('The microphone could not start. Try again.');
		expect(d.denied).toBe(false);
		await d.toggle();
		expect(d.error).toBeNull();
	});

	it('a refused partial-words listener stops the microphone and says so instead of failing silently', async () => {
		const core = await import('@tauri-apps/api/core');
		vi.mocked(core.invoke).mockResolvedValue(undefined);
		vi.mocked(core.addPluginListener).mockRejectedValue(new Error('aroh-speech.register_listener not allowed'));
		const { speech } = await import('./speech.svelte');
		await expect(speech.start(() => {})).rejects.toThrow('register_listener');
		expect(core.invoke).toHaveBeenCalledWith('plugin:aroh-speech|stop');
	});

	it('an empty transcript leaves the typed message alone', async () => {
		const seen: string[] = [];
		const d = new Dictation(fakeSpeech([], '  '), () => 'typed', (text) => seen.push(text));
		await d.toggle();
		await d.toggle();
		expect(seen).toEqual([]);
	});

	it('a second mic session after a pause appends to the draft instead of replacing it', async () => {
		let draft = 'Buy milk';
		let ended: ((text: string) => void) | undefined;
		const sessions = [['and eggs'], ['then bread']];
		const voice: Speech = {
			async start(onPartial, onEnded) {
				ended = onEnded;
				(sessions.shift() ?? []).forEach(onPartial);
			},
			async stop() {
				return 'then bread';
			}
		};
		const d = new Dictation(voice, () => draft, (text) => (draft = text));
		await d.toggle();
		expect(draft).toBe('Buy milk and eggs');
		ended?.('and eggs');
		expect(d.listening).toBe(false);
		expect(draft).toBe('Buy milk and eggs');
		await d.toggle();
		expect(d.listening).toBe(true);
		expect(draft).toBe('Buy milk and eggs then bread');
		await d.toggle();
		expect(draft).toBe('Buy milk and eggs then bread');
	});

	it('partials carry the whole session, so words heard before a pause stay in the draft', async () => {
		let draft = 'Note:';
		const d = new Dictation(fakeSpeech(['I read', 'I read two chapters', 'I read two chapters then slept'], 'I read two chapters then slept'), () => draft, (text) => (draft = text));
		await d.toggle();
		expect(draft).toBe('Note: I read two chapters then slept');
		await d.toggle();
		expect(draft).toBe('Note: I read two chapters then slept');
	});

	it('a longer utterance after a pause is not appended twice', async () => {
		let draft = '';
		const session = ['I ate eggs', 'I ate eggs then I walked the dog for an hour'];
		const d = new Dictation(fakeSpeech(session, session[1]), () => draft, (text) => (draft = text));
		await d.toggle();
		await d.toggle();
		expect(draft).toBe('I ate eggs then I walked the dog for an hour');
	});

	it('a same-first-word utterance after a pause is not appended twice', async () => {
		let draft = '';
		const session = ['I ate eggs', 'I ate eggs I walked'];
		const d = new Dictation(fakeSpeech(session, session[1]), () => draft, (text) => (draft = text));
		await d.toggle();
		await d.toggle();
		expect(draft).toBe('I ate eggs I walked');
	});

	it('a shorter revision within one utterance replaces the words instead of appending', async () => {
		const seen: string[] = [];
		const d = new Dictation(fakeSpeech(['Ice cream and tea', 'I scream'], 'I scream'), () => '', (text) => seen.push(text));
		await d.toggle();
		await d.toggle();
		expect(seen).toEqual(['Ice cream and tea', 'I scream', 'I scream']);
	});

	it('the text the recognizer ends with on its own is the whole session', async () => {
		let draft = '';
		let ended: ((text: string) => void) | undefined;
		const voice: Speech = {
			async start(onPartial, onEnded) {
				ended = onEnded;
				['I ate eggs', 'I ate eggs then slept'].forEach(onPartial);
			},
			async stop() {
				return '';
			}
		};
		const d = new Dictation(voice, () => draft, (text) => (draft = text));
		await d.toggle();
		ended?.('I ate eggs then slept');
		expect(d.listening).toBe(false);
		expect(draft).toBe('I ate eggs then slept');
	});
});

/** A speech stand-in that records calls; `start` waits on `gate` when given and rejects with `startFail` codes in turn. */
function spySpeech(opts: { startFail?: SpeechError['code'][]; gate?: Promise<void>; final?: string; stopFail?: SpeechError['code'] } = {}) {
	const calls: string[] = [];
	const fails = [...(opts.startFail ?? [])];
	const voice: Speech = {
		async start(onPartial) {
			calls.push('start');
			if (opts.gate) await opts.gate;
			const code = fails.shift();
			if (code) throw new SpeechError(code, 'Already listening.');
			onPartial('hello');
		},
		async stop() {
			calls.push('stop');
			if (opts.stopFail) throw new SpeechError(opts.stopFail, 'Voice input is not running.');
			return opts.final ?? 'hello there';
		}
	};
	return { voice, calls };
}

describe('mic sessions never block a later mic (B112)', () => {
	beforeEach(() => {
		page.visibilityState = 'visible';
	});

	it('releasing a listening mic stops its native session and keeps the words', async () => {
		let draft = '';
		const { voice, calls } = spySpeech();
		const d = new Dictation(voice, () => draft, (text) => (draft = text));
		await d.toggle();
		await d.release();
		expect(calls).toEqual(['start', 'stop']);
		expect(d.listening).toBe(false);
		expect(draft).toBe('hello there');
	});

	it('releasing an idle mic touches nothing native, and releasing twice stops once', async () => {
		const { voice, calls } = spySpeech();
		const d = new Dictation(voice, () => '', () => {});
		await d.release();
		expect(calls).toEqual([]);
		await d.toggle();
		await Promise.all([d.release(), d.release()]);
		expect(calls).toEqual(['start', 'stop']);
	});

	it('releasing while the start is still in flight stops the session once it has started', async () => {
		let open!: () => void;
		const { voice, calls } = spySpeech({ gate: new Promise((r) => (open = r)) });
		const d = new Dictation(voice, () => '', () => {});
		const started = d.toggle();
		const released = d.release();
		open();
		await Promise.all([started, released]);
		expect(calls).toEqual(['start', 'stop']);
		expect(d.listening).toBe(false);
	});

	it('a second tap while the start is in flight waits for it, then stops, with no error', async () => {
		let open!: () => void;
		const { voice, calls } = spySpeech({ gate: new Promise((r) => (open = r)) });
		const d = new Dictation(voice, () => '', () => {});
		const first = d.toggle();
		const second = d.toggle();
		open();
		await Promise.all([first, second]);
		expect(calls).toEqual(['start', 'stop']);
		expect(d.listening).toBe(false);
		expect(d.error).toBeNull();
	});

	it('a mic started on another screen releases the first one without stopping the new session', async () => {
		const a = spySpeech();
		const b = spySpeech();
		const first = new Dictation(a.voice, () => '', () => {});
		const second = new Dictation(b.voice, () => '', () => {});
		await first.toggle();
		await second.toggle();
		expect(first.listening).toBe(false);
		expect(second.listening).toBe(true);
		await first.release();
		expect(a.calls).toEqual(['start']);
		await first.toggle();
		expect(a.calls).toEqual(['start', 'start']);
		expect(second.listening).toBe(false);
	});

	it('a busy native session left by an older build is stopped and the mic starts anyway', async () => {
		const { voice, calls } = spySpeech({ startFail: ['busy'] });
		const d = new Dictation(voice, () => '', () => {});
		await d.toggle();
		expect(calls).toEqual(['start', 'stop', 'start']);
		expect(d.listening).toBe(true);
		expect(d.error).toBeNull();
	});

	it('a start the native side superseded ends quietly', async () => {
		const { voice } = spySpeech({ startFail: ['superseded'] });
		const d = new Dictation(voice, () => '', () => {});
		await d.toggle();
		expect(d.listening).toBe(false);
		expect(d.error).toBeNull();
	});

	it('stopping a session the native side already ended shows no error', async () => {
		const { voice } = spySpeech({ stopFail: 'not_started' });
		const d = new Dictation(voice, () => '', () => {});
		await d.toggle();
		await d.toggle();
		expect(d.listening).toBe(false);
		expect(d.error).toBeNull();
	});

	it('no native or raw message ever reaches the owner', async () => {
		const raw: Speech = {
			async start() {
				throw new Error('aroh-speech.register_listener not allowed');
			},
			async stop() {
				return '';
			}
		};
		const d = new Dictation(raw, () => '', () => {});
		await d.toggle();
		expect(d.error).toBe('Voice input stopped. Tap the mic to try again.');
		const busy = new Dictation(spySpeech({ startFail: ['busy', 'busy'] }).voice, () => '', () => {});
		await busy.toggle();
		expect(busy.error).toBe('Voice input stopped. Tap the mic to try again.');
		expect(busy.listening).toBe(false);
	});

	it('the app going to the background stops the listening mic and keeps its words', async () => {
		let draft = '';
		const { voice, calls } = spySpeech();
		const d = new Dictation(voice, () => draft, (text) => (draft = text));
		await d.toggle();
		page.visibilityState = 'hidden';
		page.dispatchEvent(new Event('visibilitychange'));
		await vi.waitFor(() => expect(calls).toEqual(['start', 'stop']));
		await vi.waitFor(() => expect(d.listening).toBe(false));
		expect(draft).toBe('hello there');
	});
});
