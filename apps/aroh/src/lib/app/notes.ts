import { formatDate, shiftDay } from '$lib/ui/format';
import type { PageKind, SearchHit, TreeRow } from './notebook';

/** A page tree node: the row plus its subpages, sorted by title. */
export type TreeNode = TreeRow & { children: TreeNode[] };

/** One [[ suggestion: what a picked item inserts as a pageLink, plus the label shown. */
export type LinkItem = { target_kind: PageKind; target: string; label: string; hint: string };

/** What [[ suggests from: today's local date, page search hits, actions and goals. */
export type LinkSources = { today: string; pages: SearchHit[]; actions: { id: string; title: string }[]; goals: { id: string; title: string }[] };

/** One slash menu entry: its id, label and the words it matches. */
export type SlashItem = { id: string; label: string; words: string };

/** The title a new page starts with; its title field opens selected so typing replaces it. */
export const UNTITLED = 'Untitled';

const YMD = /^\d{4}-\d{2}-\d{2}$/;
const MAX_LINKS = 12;

/** The app route of a page; free pages and pages opened by id use kind 'page'. */
export function pageHref(kind: PageKind, target: string): string {
	return `/notes/${kind}/${encodeURIComponent(target)}`;
}

/** Sorts nodes by title, then id, all the way down. */
function sortNodes(nodes: TreeNode[]): TreeNode[] {
	nodes.sort((a, b) => a.title.localeCompare(b.title) || a.id.localeCompare(b.id));
	for (const n of nodes) sortNodes(n.children);
	return nodes;
}

/**
 * The page tree from flat `rows`: subpages under their parent, sorted by title. A page whose
 * parent is missing, or that sits in a parent loop, goes to the top level. Day, action and goal
 * pages show at the top level only when they have subpages; they are reached from recents and
 * search otherwise.
 */
export function pageTree(rows: TreeRow[]): TreeNode[] {
	const nodes = new Map(rows.map((r) => [r.id, { ...r, children: [] as TreeNode[] }]));
	const roots: TreeNode[] = [];
	const reachesRoot = (id: string) => {
		const seen = new Set<string>();
		for (let at: string | null = id; at; at = nodes.get(at)?.parent_id ?? null) {
			if (seen.has(at)) return false;
			seen.add(at);
			if (!nodes.has(at)) return true;
		}
		return true;
	};
	for (const node of nodes.values()) {
		const parent = node.parent_id ? nodes.get(node.parent_id) : undefined;
		if (parent && reachesRoot(node.id)) parent.children.push(node);
		else roots.push({ ...node, parent_id: parent ? node.parent_id : null, children: node.children });
	}
	const shown = roots.filter((n) => n.kind === 'page' || n.children.length > 0);
	console.assert(shown.length <= rows.length, 'tree roots come from the rows');
	return sortNodes(shown);
}

/** The pages `pageId` can move under: every page except itself and its subpages, by title. */
export function moveTargets(rows: TreeRow[], pageId: string): TreeRow[] {
	const below = new Set([pageId]);
	for (let grew = true; grew; ) {
		grew = false;
		for (const r of rows) if (r.parent_id && below.has(r.parent_id) && !below.has(r.id)) (below.add(r.id), (grew = true));
	}
	return rows.filter((r) => !below.has(r.id)).sort((a, b) => a.title.localeCompare(b.title) || a.id.localeCompare(b.id));
}

/** The `count` most recently edited pages, newest first. */
export function recentPages(rows: TreeRow[], count: number): TreeRow[] {
	return [...rows].sort((a, b) => b.updated_at.localeCompare(a.updated_at)).slice(0, count);
}

/** True when `ymd` is a real calendar date written YYYY-MM-DD. */
export function isDate(ymd: string): boolean {
	if (!YMD.test(ymd)) return false;
	const d = new Date(`${ymd}T12:00:00Z`);
	return !Number.isNaN(d.getTime()) && d.toISOString().slice(0, 10) === ymd;
}

/** The titles a link label is looked up in. */
export type LinkNames = { rows: TreeRow[]; actions: { id: string; title: string }[]; goals: { id: string; title: string }[] };

/** The text a pageLink shows: the page title, the date, or the action or goal title, with a fallback when it is gone. */
export function linkLabel(kind: PageKind, target: string, names: LinkNames): string {
	if (kind === 'day') return isDate(target) ? formatDate(target, { weekday: 'short', day: 'numeric', month: 'short', year: 'numeric' }) : target;
	if (kind === 'action') return names.actions.find((a) => a.id === target)?.title ?? 'Action';
	if (kind === 'goal') return names.goals.find((g) => g.id === target)?.title ?? 'Goal';
	return names.rows.find((r) => r.id === target)?.title || 'Untitled page';
}

/** True when every word of `query` appears in `text`, ignoring case. */
function matches(text: string, query: string): boolean {
	const t = text.toLowerCase();
	return query.toLowerCase().split(/\s+/).filter(Boolean).every((w) => t.includes(w));
}

/**
 * The [[ suggestions for `query`: today, yesterday and tomorrow, a typed YYYY-MM-DD date, then
 * page hits, actions and goals whose title matches, at most twelve.
 */
export function linkItems(query: string, sources: LinkSources): LinkItem[] {
	const q = query.trim();
	const near: LinkItem[] = [
		{ target_kind: 'day', target: sources.today, label: 'Today', hint: sources.today },
		{ target_kind: 'day', target: shiftDay(sources.today, -1), label: 'Yesterday', hint: shiftDay(sources.today, -1) },
		{ target_kind: 'day', target: shiftDay(sources.today, 1), label: 'Tomorrow', hint: shiftDay(sources.today, 1) }
	];
	const days = near.filter((d) => matches(`${d.label} ${d.hint}`, q));
	if (isDate(q) && !days.some((d) => d.target === q)) days.push({ target_kind: 'day', target: q, label: q, hint: 'Day' });
	const pages: LinkItem[] = sources.pages.map((p) => ({ target_kind: 'page', target: p.id, label: p.title || 'Untitled', hint: p.kind === 'page' ? 'Page' : p.kind }));
	const actions: LinkItem[] = sources.actions.filter((a) => matches(a.title, q)).map((a) => ({ target_kind: 'action', target: a.id, label: a.title, hint: 'Action' }));
	const goals: LinkItem[] = sources.goals.filter((g) => matches(g.title, q)).map((g) => ({ target_kind: 'goal', target: g.id, label: g.title, hint: 'Goal' }));
	return [...days, ...pages, ...actions, ...goals].slice(0, MAX_LINKS);
}

const SLASH: SlashItem[] = [
	{ id: 'paragraph', label: 'Text', words: 'text paragraph plain' },
	{ id: 'h1', label: 'Heading 1', words: 'heading title h1 big' },
	{ id: 'h2', label: 'Heading 2', words: 'heading subtitle h2' },
	{ id: 'h3', label: 'Heading 3', words: 'heading h3 small' },
	{ id: 'bullet', label: 'Bulleted list', words: 'bullet list unordered' },
	{ id: 'numbered', label: 'Numbered list', words: 'numbered list ordered' },
	{ id: 'check', label: 'Checklist', words: 'checklist todo task check' },
	{ id: 'quote', label: 'Quote', words: 'quote blockquote' },
	{ id: 'divider', label: 'Divider', words: 'divider rule line separator' },
	{ id: 'link', label: 'Link to page', words: 'link page date action goal' }
];

/** The slash menu entries whose label or words match `query`. */
export function slashItems(query: string): SlashItem[] {
	return SLASH.filter((i) => matches(`${i.label} ${i.words}`, query));
}
