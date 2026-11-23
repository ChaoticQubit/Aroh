/** The window parts `fitKeyboard` uses, so a test can pass a stand-in. */
export type KeyboardWindow = Pick<Window, 'addEventListener' | 'removeEventListener'> & {
	visualViewport: Pick<VisualViewport, 'height' | 'addEventListener' | 'removeEventListener'> | null;
	document: {
		activeElement: unknown;
		documentElement: { style: Pick<CSSStyleDeclaration, 'setProperty'>; toggleAttribute(name: string, force: boolean): boolean };
	};
};

const TEXT_FIELD = 'input, textarea, [contenteditable="true"]';

/** Whether `el` takes typed text, so the software keyboard covers the bottom of the screen. */
function typesText(el: unknown): el is Element {
	return !!el && typeof (el as Element).matches === 'function' && (el as Element).matches(TEXT_FIELD);
}

/**
 * Keeps the app in the part of the screen the software keyboard leaves free, like a native app:
 * `--app-h` follows the visual viewport height, `data-typing` on the root marks a focused text
 * field (CSS hides the tab bar and pinned bars then), and once the keyboard settles the focused
 * field scrolls into view inside its own scroller. It never scrolls the page itself; the native
 * side holds the web view's page offset at zero. Returns the function that stops it.
 */
export function fitKeyboard(win: KeyboardWindow): () => void {
	const vv = win.visualViewport;
	if (!vv) return () => {};
	const root = win.document.documentElement;
	const fit = () => {
		root.style.setProperty('--app-h', `${Math.round(vv.height)}px`);
		const field = win.document.activeElement;
		if (typesText(field)) field.scrollIntoView({ block: 'nearest' });
	};
	const focusin = (event: Event) => root.toggleAttribute('data-typing', typesText(event.target));
	const focusout = () => root.toggleAttribute('data-typing', false);
	fit();
	vv.addEventListener('resize', fit);
	win.addEventListener('focusin', focusin);
	win.addEventListener('focusout', focusout);
	return () => {
		vv.removeEventListener('resize', fit);
		win.removeEventListener('focusin', focusin);
		win.removeEventListener('focusout', focusout);
		root.toggleAttribute('data-typing', false);
	};
}
