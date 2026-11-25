/** What `atEnd` watches: in the parent list when `own` (it scrolls itself) else in the page, less the `inset` px a sticky composer covers. */
export type AtEnd = { own: boolean; inset: number; onchange: (atEnd: boolean) => void };

/**
 * Svelte action for a sentinel after the last chat entry: reports whether the owner sees the end
 * of the thread. Reports true without IntersectionObserver, so the chat then always follows.
 */
export function atEnd(el: Element, opts: AtEnd) {
	console.assert(opts.inset >= 0, 'a composer covers zero or more px');
	if (typeof IntersectionObserver === 'undefined') {
		opts.onchange(true);
		return {};
	}
	let observer: IntersectionObserver | null = null;
	const watch = (o: AtEnd) => {
		observer?.disconnect();
		const root = o.own ? el.parentElement : null;
		observer = new IntersectionObserver((entries) => o.onchange(entries.at(-1)?.isIntersecting ?? true), { root, rootMargin: `0px 0px -${o.inset}px 0px` });
		observer.observe(el);
	};
	watch(opts);
	return {
		update: watch,
		destroy: () => observer?.disconnect()
	};
}
