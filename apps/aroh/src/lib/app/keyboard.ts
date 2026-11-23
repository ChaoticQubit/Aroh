/** The window parts `fitKeyboard` uses, so a test can pass a stand-in. */
export type KeyboardWindow = Pick<Window, 'addEventListener' | 'removeEventListener' | 'scrollTo' | 'setTimeout'> & {
	visualViewport: (Pick<VisualViewport, 'height' | 'addEventListener' | 'removeEventListener'>) | null;
	document: { documentElement: { style: Pick<CSSStyleDeclaration, 'setProperty'> } };
};

/** How long iOS takes to slide the keyboard in, in milliseconds. */
const KEYBOARD_MS = 300;

/**
 * Keeps the app inside the part of the screen the software keyboard leaves free: sets `--app-h`
 * to the visual viewport height, undoes the page shift WKWebView makes, and scrolls a focused
 * field into view once the keyboard is up. Returns the function that stops it.
 */
export function fitKeyboard(win: KeyboardWindow): () => void {
	const vv = win.visualViewport;
	if (!vv) return () => {};
	const root = win.document.documentElement.style;
	const fit = () => {
		root.setProperty('--app-h', `${Math.round(vv.height)}px`);
		win.scrollTo(0, 0);
	};
	const focus = (event: Event) => {
		const field = event.target as Element | null;
		if (!field || !('matches' in field) || !field.matches('input, textarea, [contenteditable="true"]')) return;
		win.setTimeout(() => field.scrollIntoView({ block: 'nearest' }), KEYBOARD_MS);
	};
	fit();
	vv.addEventListener('resize', fit);
	win.addEventListener('focusin', focus);
	return () => {
		vv.removeEventListener('resize', fit);
		win.removeEventListener('focusin', focus);
	};
}
