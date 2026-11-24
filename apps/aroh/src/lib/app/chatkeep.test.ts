import { describe, expect, it } from 'vitest';
import { clearDraft, MAX_KEPT, MAX_TEXT, readDraft, readSent, rememberSent, writeDraft, type Keep } from './chatkeep';

function memory(): Keep & { data: Map<string, string> } {
	const data = new Map<string, string>();
	return { data, getItem: (k) => data.get(k) ?? null, setItem: (k, v) => void data.set(k, v), removeItem: (k) => void data.delete(k) };
}

describe('chat drafts and sent history (B60, B62)', () => {
	it('keeps an unsent draft per chat until it is sent, spaces and new lines included', () => {
		const store = memory();
		writeDraft(store, 'goal:g1', '  run on\nMonday  ');
		writeDraft(store, 'card:a1', 'lighter sets');
		expect(readDraft(store, 'goal:g1')).toBe('  run on\nMonday  ');
		expect(readDraft(store, 'card:a1')).toBe('lighter sets');
		expect(readDraft(store, 'goal:g2')).toBe('');
		clearDraft(store, 'goal:g1');
		expect(readDraft(store, 'goal:g1')).toBe('');
		writeDraft(store, 'card:a1', '');
		expect(store.data.has('aroh.draft.card:a1')).toBe(false);
	});

	it('caps a draft and survives a store that throws or holds junk', () => {
		const store = memory();
		writeDraft(store, 'goal:g1', 'x'.repeat(MAX_TEXT + 50));
		expect(readDraft(store, 'goal:g1').length).toBe(MAX_TEXT);
		const broken: Keep = {
			getItem: () => {
				throw new Error('denied');
			},
			setItem: () => {
				throw new Error('full');
			},
			removeItem: () => {
				throw new Error('denied');
			}
		};
		expect(() => writeDraft(broken, 'goal:g1', 'hi')).not.toThrow();
		expect(readDraft(broken, 'goal:g1')).toBe('');
		expect(readSent(broken, 'g1')).toEqual([]);
		store.data.set('aroh.sent.g1', '{not json');
		expect(readSent(store, 'g1')).toEqual([]);
		store.data.set('aroh.sent.g1', JSON.stringify([{ text: 3 }, { text: 'ok', at: 1 }]));
		expect(readSent(store, 'g1')).toEqual([{ text: 'ok', at: 1 }]);
	});

	it('remembers sent owner messages per goal in order, keeping the newest', () => {
		const store = memory();
		rememberSent(store, 'g1', 'first', 1);
		rememberSent(store, 'g1', 'second', 2);
		rememberSent(store, 'g2', 'other goal', 3);
		expect(readSent(store, 'g1')).toEqual([
			{ text: 'first', at: 1 },
			{ text: 'second', at: 2 }
		]);
		for (let i = 0; i < MAX_KEPT + 5; i++) rememberSent(store, 'g3', `m${i}`, i);
		const kept = readSent(store, 'g3');
		expect(kept.length).toBe(MAX_KEPT);
		expect(kept.at(-1)?.text).toBe(`m${MAX_KEPT + 4}`);
	});
});
