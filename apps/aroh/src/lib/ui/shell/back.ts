/** How a navigation happened, as SvelteKit's `afterNavigate` reports it. */
export type NavKind = 'enter' | 'form' | 'leave' | 'link' | 'goto' | 'popstate';

/** Where the top bar's back arrow goes: its label, whether it is a history step, and the path. */
export type Back = { label: string; history: boolean; path: string };

const ROOTS: Record<string, string> = { '/today': 'Today', '/calendar': 'Calendar', '/goals': 'Goals', '/dashboard': 'Dashboard', '/log': 'Log', '/settings': 'Settings', '/inbox': 'Inbox' };
const SUBS: Record<string, string> = { '/goals/new': 'New goal', '/settings/location': 'Location', '/settings/location/log': 'Location log', '/settings/places': 'Places', '/settings/profile': 'What Aroh knows', '/settings/key': 'Claude key', '/settings/agents': 'Agent defaults', '/settings/delete': 'Delete account' };

/** Splits "/path?query" into its path and query. */
function split(url: string): [string, URLSearchParams] {
	const at = url.indexOf('?');
	return at < 0 ? [url, new URLSearchParams()] : [url.slice(0, at), new URLSearchParams(url.slice(at + 1))];
}

/** The title of the screen at `url` ("/path?query"), as the top bar and a back label show it. */
export function screenTitle(url: string): string {
	const [path] = split(url);
	if (ROOTS[path]) return ROOTS[path];
	if (SUBS[path]) return SUBS[path];
	if (path.startsWith('/personas')) return 'Coaches';
	if (/^\/goals\/[^/]+\/chat$/.test(path)) return 'Goal chat';
	if (/^\/actions\/[^/]+\/chat$/.test(path)) return 'Card chat';
	if (/^\/actions\/[^/]+$/.test(path)) return 'Session';
	if (/^\/goals\/[^/]+$/.test(path)) return 'Goal';
	return 'Aroh';
}

/**
 * The screen above `url` in the app's tree, for a back arrow with no in-app history: null on the
 * tab roots, Settings and Inbox. Coaches opened from a goal (`?goal=`) belong to that goal.
 */
export function parentOf(url: string): { path: string; label: string } | null {
	const [path, query] = split(url);
	if (ROOTS[path]) return null;
	const up = (p: string) => ({ path: p, label: screenTitle(p) });
	if (path.startsWith('/personas')) return query.get('goal') ? up(`/goals/${query.get('goal')}`) : up('/settings');
	if (path === '/settings/location/log') return up('/settings/location');
	if (path.startsWith('/settings/')) return up('/settings');
	const chat = /^(\/(?:goals|actions)\/[^/]+)\/chat$/.exec(path);
	if (chat) return up(chat[1]);
	if (path.startsWith('/goals/')) return up('/goals');
	return up('/today');
}

/**
 * The in-app history after a navigation to `url`: entry restarts it, links and gotos push, a
 * browser back pops when it lands on the previous entry, any other popstate restarts from `url`.
 */
export function nextStack(stack: string[], url: string, kind: NavKind): string[] {
	if (kind === 'enter' || !stack.length) return [url];
	if (stack[stack.length - 1] === url) return stack;
	if (kind === 'popstate') return stack.length > 1 && stack[stack.length - 2] === url ? stack.slice(0, -1) : [url];
	return [...stack, url];
}

/**
 * The back arrow for the screen on top of `stack`: none on a root; else the previous screen when
 * there is in-app history (a history step, so its scroll comes back), or the parent on a cold start.
 */
export function backTarget(stack: string[]): Back | null {
	const here = stack[stack.length - 1];
	console.assert(here !== undefined, 'back needs a current screen');
	const parent = parentOf(here);
	if (!parent) return null;
	if (stack.length > 1) return { label: screenTitle(stack[stack.length - 2]), history: true, path: stack[stack.length - 2] };
	return { label: parent.label, history: false, path: parent.path };
}
