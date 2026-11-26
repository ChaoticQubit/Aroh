import { afterEach, describe, expect, it, vi } from 'vitest';
import { follow, toEnd, type Field } from './follow';
import { grow } from './grow';

/** A text field as the follower sees it: a value, scroll box, caret and input events, focused or not. */
function fakeField(value: string, focused = true) {
	const events = new EventTarget();
	const doc = { activeElement: null as unknown };
	const field = {
		value,
		scrollTop: 0,
		scrollHeight: 600,
		scrollLeft: 0,
		scrollWidth: 900,
		selection: null as [number, number] | null,
		style: {} as Record<string, string>,
		ownerDocument: doc,
		setSelectionRange(start: number, end: number) {
			field.selection = [start, end];
		},
		addEventListener: events.addEventListener.bind(events),
		removeEventListener: events.removeEventListener.bind(events),
		dispatchEvent: events.dispatchEvent.bind(events)
	};
	if (focused) doc.activeElement = field;
	return field;
}

/** Lets queued microtasks run, as Svelte's action updates do. */
const settle = () => new Promise<void>((done) => queueMicrotask(done));

describe('dictation keeps the latest words in view (B125)', () => {
	afterEach(() => vi.unstubAllGlobals());

	it('moves the caret and scroll to the end of the text', () => {
		const f = fakeField('I want to run a marathon in spring');
		toEnd(f as unknown as Field);
		expect(f.selection).toEqual([f.value.length, f.value.length]);
		expect(f.scrollTop).toBe(600);
		expect(f.scrollLeft).toBe(900);
	});

	it('scrolls an unfocused box without placing a caret, so the keyboard stays down', () => {
		const f = fakeField('words', false);
		toEnd(f as unknown as Field);
		expect(f.selection).toBeNull();
		expect(f.scrollTop).toBe(600);
	});

	it('follows each dictated partial that replaces the value', async () => {
		const f = fakeField('');
		const action = follow(f as unknown as Field, '');
		for (const partial of ['I want', 'I want to run', 'I want to run a marathon in spring']) {
			f.value = partial;
			f.scrollTop = 0;
			action.update(partial);
			await settle();
			expect(f.scrollTop).toBe(600);
			expect(f.selection).toEqual([partial.length, partial.length]);
		}
		action.destroy();
	});

	it('leaves the caret where the owner types', async () => {
		const f = fakeField('I want to run');
		const action = follow(f as unknown as Field, f.value);
		f.value = 'I really want to run';
		f.dispatchEvent(new Event('input'));
		action.update(f.value);
		await settle();
		expect(f.selection).toBeNull();
		expect(f.scrollTop).toBe(0);
		action.destroy();
	});

	it('an auto-growing composer fits its height first, then shows the last words', async () => {
		vi.stubGlobal('getComputedStyle', () => ({ lineHeight: '28px', fontSize: '18px', paddingTop: '8px', paddingBottom: '8px', borderTopWidth: '1px', borderBottomWidth: '1px' }));
		const f = fakeField('');
		const action = grow(f as unknown as HTMLTextAreaElement, '');
		await settle();
		f.value = 'one two three four five six seven eight nine ten eleven twelve';
		action.update();
		await settle();
		expect(f.style.overflowY).toBe('auto');
		expect(f.scrollTop).toBe(600);
		expect(f.selection).toEqual([f.value.length, f.value.length]);
		action.destroy();
	});
});
