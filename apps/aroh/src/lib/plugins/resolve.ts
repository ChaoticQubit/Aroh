import type { Clock } from '$lib/ui/clock';
import type { ComponentSpec, Point, ViewSpec } from '$lib/ui/contract';
import { formatYmd, mondayOf, shiftDay } from '$lib/ui/format';
import type { Goal, Snapshot, Tracker } from '$lib/app/client';
import { actionViews, deadlineAt, entryView, nextPlanned, points, streak, weekActions } from '$lib/app/views';

/** Every binding source the resolver computes, in the order the Rust crate lists them. */
export const SOURCES = ['series', 'latest', 'today_total', 'change', 'target', 'entries', 'streak', 'actions', 'week_done', 'week_planned', 'week_start', 'deadline'] as const;

/** One binding source name. */
export type Source = (typeof SOURCES)[number];

/** A binding as the validated tree carries it: the source and the keys the Rust crate checked. */
export type Binding = { bind: string; tracker?: string; field?: string; when?: 'next' | 'today' | 'week'; limit?: number };

/** One tracker a plugin declares, with a target per number field that has one. */
export type TreeTracker = { name: string; fields: { key: string; label: string; kind: string; unit: string | null; target?: number }[] };

/** A validated plugin tree, as `aroh_plugins::parse` emits it: items keep their bindings. */
export type PluginTree = {
	format: 1;
	name: string;
	title: string;
	description: string | null;
	trackers: TreeTracker[];
	tools: string[];
	personas: string[];
	sections: { title: string; items: unknown[] }[];
};

const NOTHING = Symbol('nothing');
const ACTION_LIMIT = 50;
const ENTRY_LIMIT = 20;
const STREAK_DAYS = 28;
const CHANGE_DAYS = 7;

type Ctx = { s: Snapshot; goal: Goal; clock: Clock; tree: PluginTree; today: string };
type Resolved = unknown | typeof NOTHING;

/**
 * Checks the shape of a tree that came from the Rust validator: format 1 with sections.
 * Input: parsed JSON. Output: the tree, or a thrown error for anything else.
 */
export function parseTree(json: unknown): PluginTree {
	const t = json as PluginTree | null;
	if (!t || t.format !== 1 || !Array.isArray(t.sections) || !Array.isArray(t.trackers)) throw new Error('unsupported plugin tree format');
	return t;
}

function trackerRow(c: Ctx, b: Binding): Tracker | undefined {
	return c.s.trackers.find((t) => t.goal_id === c.goal.id && t.name === b.tracker);
}

function series(c: Ctx, b: Binding): Point[] {
	const row = trackerRow(c, b);
	return row && b.field ? points(c.s, row.id, b.field) : [];
}

function goalActions(c: Ctx) {
	return c.s.actions.filter((a) => a.goal_id === c.goal.id);
}

/** What each source gives for a binding, or NOTHING when there is no data to show. */
const RESOLVERS: Record<Source, (b: Binding, c: Ctx) => Resolved> = {
	series: (b, c) => {
		const p = series(c, b);
		return p.length ? p : NOTHING;
	},
	latest: (b, c) => series(c, b).at(-1)?.value ?? NOTHING,
	today_total: (b, c) => series(c, b).filter((p) => formatYmd(p.at, c.clock.zone) === c.today).reduce((sum, p) => sum + p.value, 0),
	change: (b, c) => {
		const since = shiftDay(c.today, -(CHANGE_DAYS - 1));
		const p = series(c, b).filter((x) => formatYmd(x.at, c.clock.zone) >= since);
		return p.length ? p[p.length - 1].value - p[0].value : NOTHING;
	},
	target: (b, c) => c.tree.trackers.find((t) => t.name === b.tracker)?.fields.find((f) => f.key === b.field)?.target ?? NOTHING,
	entries: (b, c) => {
		const row = trackerRow(c, b);
		return row ? c.s.entries.filter((e) => e.tracker_id === row.id).slice(0, ENTRY_LIMIT).map((e) => entryView(e, row)) : [];
	},
	streak: (b, c) => {
		if (b.tracker === undefined) {
			const actions = goalActions(c);
			return actions.length ? streak(actions, c.today, c.clock.zone) : NOTHING;
		}
		const row = trackerRow(c, b);
		const logged = new Set(row ? c.s.entries.filter((e) => e.tracker_id === row.id).map((e) => formatYmd(e.logged_at, c.clock.zone)) : []);
		if (!logged.size) return NOTHING;
		return Array.from({ length: STREAK_DAYS }, (_, i) => {
			const date = shiftDay(c.today, i - (STREAK_DAYS - 1));
			return { date, level: logged.has(date) ? 3 : 0 };
		});
	},
	actions: (b, c) => {
		const views = actionViews({ ...c.s, actions: goalActions(c) });
		const limit = b.limit ?? ACTION_LIMIT;
		if (b.when === 'next') return nextPlanned(views, c.clock, limit);
		const ids = new Set((b.when === 'week' ? weekActions(goalActions(c), c.clock) : goalActions(c).filter((a) => formatYmd(a.start_at, c.clock.zone) === c.today)).map((a) => a.id));
		return views.filter((v) => ids.has(v.id)).slice(0, limit);
	},
	week_done: (_b, c) => weekActions(goalActions(c), c.clock).filter((a) => a.status === 'done').length,
	week_planned: (_b, c) => weekActions(goalActions(c), c.clock).length || NOTHING,
	week_start: (_b, c) => mondayOf(c.today),
	deadline: (_b, c) => (c.goal.deadline ? deadlineAt(c.goal.deadline, c.clock.zone) : NOTHING)
};

function isSource(name: string): name is Source {
	return (SOURCES as readonly string[]).includes(name);
}

function value(v: unknown, c: Ctx): Resolved {
	if (Array.isArray(v)) {
		const out = v.map((x) => value(x, c));
		return out.includes(NOTHING) ? NOTHING : out;
	}
	if (!v || typeof v !== 'object') return v;
	if ('bind' in v) {
		const b = v as Binding;
		if (!isSource(b.bind)) throw new Error(`unknown binding source \`${b.bind}\``);
		return RESOLVERS[b.bind](b, c);
	}
	const out: Record<string, unknown> = {};
	for (const [k, x] of Object.entries(v)) {
		const r = value(x, c);
		if (r === NOTHING) return NOTHING;
		out[k] = r;
	}
	return out;
}

/**
 * Resolves a plugin tree against the snapshot for `goal` at `clock`: every binding becomes data,
 * an item with a value that resolves to nothing is left out, and so is a section with no items
 * left. The view is titled by the goal. Throws on an unknown binding source.
 */
export function resolve(tree: PluginTree, s: Snapshot, goal: Goal, clock: Clock): ViewSpec {
	const c: Ctx = { s, goal, clock, tree, today: formatYmd(clock.now, clock.zone) };
	const sections = tree.sections.map((sec) => ({
		title: sec.title,
		items: sec.items.map((item) => value(item, c)).filter((item): item is ComponentSpec => item !== NOTHING)
	}));
	return { title: goal.title, sections: sections.filter((sec) => sec.items.length > 0) };
}
