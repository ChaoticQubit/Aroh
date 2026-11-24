/** Lines a composer grows to before it scrolls inside. */
export const GROW_LINES = 6;

/**
 * The height a composer takes for content `scroll` px tall, with `line` px lines and `pad` px of
 * vertical padding and border: at least one line, at most `GROW_LINES`; `scroll` is true past that.
 */
export function grownHeight(content: number, line: number, pad: number): { height: number; scroll: boolean } {
	console.assert(line > 0 && pad >= 0, 'a composer has a line height and padding');
	const max = GROW_LINES * line + pad;
	const height = Math.min(Math.max(content, line + pad), max);
	return { height, scroll: content > max };
}

/**
 * Svelte action for a textarea: fits its height to the text on every input and whenever the
 * bound `value` changes (dictation writes it), up to `GROW_LINES`, then scrolls inside.
 */
export function grow(el: HTMLTextAreaElement, _value?: string) {
	const fit = () => {
		const style = getComputedStyle(el);
		const line = parseFloat(style.lineHeight) || parseFloat(style.fontSize) * 1.4;
		const pad = parseFloat(style.paddingTop) + parseFloat(style.paddingBottom) + parseFloat(style.borderTopWidth) + parseFloat(style.borderBottomWidth);
		el.style.height = 'auto';
		const next = grownHeight(el.scrollHeight + parseFloat(style.borderTopWidth) + parseFloat(style.borderBottomWidth), line, pad);
		el.style.height = `${next.height}px`;
		el.style.overflowY = next.scroll ? 'auto' : 'hidden';
	};
	el.addEventListener('input', fit);
	queueMicrotask(fit);
	return {
		update: () => queueMicrotask(fit),
		destroy: () => el.removeEventListener('input', fit)
	};
}
