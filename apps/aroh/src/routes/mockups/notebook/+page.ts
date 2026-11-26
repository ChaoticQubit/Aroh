export const prerender = false;
export const ssr = false;

/** Shows the notebook mockup without the app shell. Returns `{ bare: true }`. */
export function load() {
	return { bare: true };
}
