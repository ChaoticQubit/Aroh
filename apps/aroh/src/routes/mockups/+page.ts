export const prerender = true;
export const ssr = true;

/**
 * Marks the page as bare so the root layout renders it without the app shell.
 * The flag travels in page data, not the URL, because the exported single file
 * is served from an unknown path. Returns `{ bare: true }`.
 */
export function load() {
	return { bare: true };
}
