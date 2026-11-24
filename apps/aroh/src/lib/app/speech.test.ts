import { describe, expect, it, vi } from 'vitest';

vi.mock('@tauri-apps/api/core', () => ({ invoke: vi.fn(), addPluginListener: vi.fn() }));

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
		const d = new Dictation(fakeSpeech(['I read', 'I read two'], 'I read two chapters'), (text) => seen.push(text));
		await d.toggle();
		expect(d.listening).toBe(true);
		expect(seen).toEqual(['I read', 'I read two']);
		await d.toggle();
		expect(d.listening).toBe(false);
		expect(seen.at(-1)).toBe('I read two chapters');
		expect(d.error).toBeNull();
	});

	it('a denied microphone says how to turn it on, offers Settings, and is not left listening', async () => {
		const d = new Dictation(fakeSpeech([], '', { at: 'start', code: 'mic_denied' }), () => {});
		await d.toggle();
		expect(d.listening).toBe(false);
		expect(d.error).toBe('Microphone is off for Aroh. Turn it on in Settings.');
		expect(d.denied).toBe(true);
	});

	it('denied speech recognition offers Settings too', async () => {
		const d = new Dictation(fakeSpeech([], '', { at: 'start', code: 'speech_denied' }), () => {});
		await d.toggle();
		expect(d.error).toBe('Speech recognition is off for Aroh. Turn it on in Settings.');
		expect(d.denied).toBe(true);
	});

	it('any other failure shows its reason without the Settings button', async () => {
		const d = new Dictation(fakeSpeech([], '', { at: 'stop', code: 'audio_failed' }), () => {});
		await d.toggle();
		await d.toggle();
		expect(d.error).toBe('Nothing was heard.');
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
		const d = new Dictation(fakeSpeech([], '  '), (text) => seen.push(text));
		await d.toggle();
		await d.toggle();
		expect(seen).toEqual([]);
	});
});
