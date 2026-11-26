/** The parts of an input or textarea the dictation follower reads and moves. */
export type Field = Pick<HTMLTextAreaElement, 'value' | 'scrollTop' | 'scrollHeight' | 'scrollLeft' | 'scrollWidth' | 'setSelectionRange' | 'addEventListener' | 'removeEventListener'> & { ownerDocument: { activeElement: unknown } | null };

/**
 * Shows the end of `el`'s text: scrolls to the last line (textarea) or last character (input) and,
 * when `el` has focus, puts the caret after the last character. An unfocused box gets no caret, so
 * the iPhone keyboard never opens for it.
 */
export function toEnd(el: Field): void {
	const end = el.value.length;
	if (el.ownerDocument?.activeElement === el) el.setSelectionRange(end, end);
	el.scrollTop = el.scrollHeight;
	el.scrollLeft = el.scrollWidth;
}

/**
 * Tracks what the owner typed into `el`. `typed` records the value on each input event; `sync`
 * calls `toEnd` when the value changed some other way, such as a dictated partial replacing it,
 * so the newest words stay in view while typing keeps its caret.
 */
export function endFollower(el: Field): { typed: () => void; sync: () => void } {
	let typed = el.value;
	return {
		typed: () => {
			typed = el.value;
		},
		sync: () => {
			if (el.value === typed) return;
			typed = el.value;
			toEnd(el);
		}
	};
}

/**
 * Svelte action for a dictation box that does not auto-grow: pass the bound value. After each
 * value change that did not come from typing, the caret and scroll move to the end.
 */
export function follow(el: Field, _value?: string) {
	const end = endFollower(el);
	el.addEventListener('input', end.typed);
	return {
		update: (_next?: string) => queueMicrotask(end.sync),
		destroy: () => el.removeEventListener('input', end.typed)
	};
}
