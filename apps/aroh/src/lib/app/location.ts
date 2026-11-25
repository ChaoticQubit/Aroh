import { invoke } from '@tauri-apps/api/core';
import { listen } from '@tauri-apps/api/event';
import type { LocationSettingsView, PlacePromptView, PlaceResult, PlaceRow, RoutineRow, VisitRow } from '$lib/ui/components/location/types';
import { formatYmd } from '$lib/ui/format';
import type { InboxItem } from '$lib/ui/screens/types';
import type { Goal, Snapshot } from './client';
import { categoryViews } from './views';

/** Everything the Rust `app_location_view` returns; raw visits stay on this phone. */
export type LocationView = {
	routines_goal: string;
	settings: { enabled: boolean; paused: boolean; nudges: boolean; watched: number; visits: number; ignored: { id: string; name: string; address: string | null }[] };
	prompts: { id: string; guess: string | null; name: string; address: string | null; visits: number; weeks: number; usual: string; lat: number; lon: number }[];
	patterns: { id: string; title: string; place: string; gap_days: number; weekday: string | null; local_time: string; duration_minutes: number; evidence: number; goal: string }[];
	places: { id: string; name: string; address: string | null; lat: number; lon: number; watched: boolean; visits: number; links: { goal_id: string; series_id: string | null }[] }[];
	routines: { series_id: string; title: string; place: string | null; cadence: string; streak: string; last_done: string | null; next: { start_at: string; status: 'planned' | 'missed' } | null }[];
	log: { id: string; place: string; arrived_at: string; left_at: string | null }[];
};

/** A MapKit search hit or reverse lookup. */
export type PlaceHit = { name: string | null; kind: string | null; address: string | null; lat: number; lon: number };

/** The owner's answer to a place prompt as Rust reads it. */
export type PlaceDecision = { kind: 'link'; goal_id: string; series_id: string | null } | { kind: 'rename'; name: string } | { kind: 'ignore' };

/** Pattern cards carry this id prefix, so the Inbox routes them to the location commands. */
export const PATTERN_PREFIX = 'pattern:';

const GRID = { w: 360, h: 200, pad: 20 };

/** Reads every location row the screens show. */
export const view = () => invoke<LocationView>('app_location_view');

/**
 * Loads the location view into `set` now and after every `aroh://changed`; errors go to
 * `fail`. Returns the function that stops listening.
 */
export function follow(set: (v: LocationView) => void, fail: (e: unknown) => void): () => void {
	const load = () => view().then(set, fail);
	void load();
	const unlisten = listen('aroh://changed', () => void load());
	return () => void unlisten.then((stop) => stop());
}

/** Asks iOS for location in two steps and turns it on; returns the status line. */
export const enable = () => invoke<string>('app_location_enable');

/** Turns location off. */
export const disable = () => invoke<null>('app_location_disable');

/** Sets the pause or nudges switch. */
export const setSetting = (name: 'location_paused' | 'location_nudges', value: boolean) => invoke<null>('app_location_setting', { name, value });

/** Answers a place prompt. */
export const decidePrompt = (id: string, decision: PlaceDecision) => invoke<unknown>('app_location_decide_prompt', { id, decision });

/** Stops ignoring a spot. */
export const unignore = (id: string) => invoke<null>('app_location_unignore', { id });

/** Searches MapKit near the device. */
export const search = (query: string) => invoke<PlaceHit[]>('app_location_search', { query });

/** A map image of a spot as a PNG data URL. */
export const snapshot = (lat: number, lon: number, width: number, height: number) => invoke<string>('app_location_snapshot', { lat, lon, width, height });

/** Names a place found by search. */
export const addPlace = (hit: PlaceHit) => invoke<unknown>('app_location_add_place', { name: hit.name ?? hit.address ?? 'Place', address: hit.address, lat: hit.lat, lon: hit.lon });

/** Applies or declines a learned pattern. */
export const decidePattern = (id: string, apply: boolean) => invoke<unknown>('app_location_decide_pattern', { id, apply });

/** Clears the location history on this phone. */
export const clear = () => invoke<null>('app_location_clear');

function liveGoals(s: Snapshot, routinesId: string): Goal[] {
	return s.goals.filter((g) => g.id !== routinesId && (g.status === 'active' || g.status === 'paused'));
}

function seriesOf(s: Snapshot): Map<string, { goalId: string; title: string }> {
	const out = new Map<string, { goalId: string; title: string }>();
	for (const a of s.actions) if (a.series_id && !out.has(a.series_id)) out.set(a.series_id, { goalId: a.goal_id, title: a.title });
	return out;
}

/**
 * The Inbox place prompts: each routine of an active goal, each active goal and the Routines
 * goal as link targets. A prompt with no guessed kind asks "Is this <name>?".
 */
export function placePrompts(v: LocationView, s: Snapshot): PlacePromptView[] {
	const goals = liveGoals(s, v.routines_goal);
	const series = seriesOf(s);
	const targets = [
		...goals.flatMap((g) => [...series].filter(([, x]) => x.goalId === g.id).map(([id, x]) => ({ value: `series:${g.id}:${id}`, label: `${x.title}, ${g.title}` }))),
		...goals.map((g) => ({ value: `goal:${g.id}`, label: `${g.title}, any action` })),
		{ value: `goal:${v.routines_goal}`, label: 'Routines' }
	];
	return v.prompts.map((p) => ({
		id: p.id,
		guess: p.guess ?? 'place',
		question: p.guess ? undefined : `Is this ${p.name}?`,
		name: p.name,
		address: p.address ?? '',
		visits: p.visits,
		weeks: p.weeks,
		usual: p.usual,
		targets
	}));
}

/** The Rust decision for a place card answer, or null for an answer it cannot be. */
export function decisionFor(answer: unknown): PlaceDecision | null {
	const a = answer as { kind?: string; target?: string; name?: string } | null;
	if (a?.kind === 'ignore') return { kind: 'ignore' };
	if (a?.kind === 'rename' && typeof a.name === 'string') return { kind: 'rename', name: a.name };
	if (a?.kind !== 'link' || typeof a.target !== 'string') return null;
	const [kind, goal, series] = a.target.split(':');
	if (kind === 'goal' && goal && series === undefined) return { kind: 'link', goal_id: goal, series_id: null };
	if (kind === 'series' && goal && series) return { kind: 'link', goal_id: goal, series_id: series };
	return null;
}

/** Learned patterns as proposal cards, built here from local rows: none is a proposal row. */
export function patternItems(v: LocationView): InboxItem[] {
	return v.patterns.map((p) => {
		const when = p.weekday ? `usually ${p.weekday} ~${p.local_time}` : `usually ~${p.local_time}`;
		const every = p.weekday ? `Every ${p.weekday} ${p.local_time}` : p.gap_days === 1 ? `Every day ${p.local_time}` : `Every ${p.gap_days} days ${p.local_time}`;
		const kind = p.gap_days === 1 ? 'daily' : p.gap_days % 7 === 0 ? 'weekly' : 'repeating';
		return {
			id: PATTERN_PREFIX + p.id,
			spec: {
				kind: 'proposal_card',
				persona: 'Planner',
				summary: `You ${p.title.toLowerCase()} at ${p.place} about every ${p.gap_days} ${p.gap_days === 1 ? 'day' : 'days'}, ${when}. Make it a ${kind} action?`,
				changes: [{ op: 'create', title: `${p.title} at ${p.place}, ${p.goal}`, before: null, after: `${every}, ${p.duration_minutes} min` }]
			}
		};
	});
}

function grid(points: { lat: number; lon: number }[]): (p: { lat: number; lon: number }) => { x: number; y: number } {
	const lats = points.map((p) => p.lat);
	const lons = points.map((p) => p.lon);
	const [south, north, west, east] = [Math.min(...lats), Math.max(...lats), Math.min(...lons), Math.max(...lons)];
	const span = (lo: number, hi: number, at: number, size: number) => (hi > lo ? GRID.pad + ((at - lo) / (hi - lo)) * (size - 2 * GRID.pad) : size / 2);
	return (p) => ({ x: Math.round(span(west, east, p.lon, GRID.w)), y: Math.round(GRID.h - span(south, north, p.lat, GRID.h)) });
}

/** Named places with linked goals and routines, watched state and a position on the map grid. */
export function placeRows(v: LocationView, s: Snapshot): PlaceRow[] {
	const categories = categoryViews(s);
	const goals = new Map(s.goals.map((g) => [g.id, g]));
	const series = seriesOf(s);
	const at = grid(v.places);
	return v.places.map((p) => ({
		id: p.id,
		name: p.name,
		address: p.address ?? '',
		links: p.links.map((l) => {
			const goal = goals.get(l.goal_id);
			const action = l.series_id ? series.get(l.series_id)?.title : undefined;
			const slot = (goal?.category_id && categories.get(goal.category_id)?.slot) || 6;
			return action ? { goal: goal?.title ?? 'Routines', slot, action } : { goal: goal?.title ?? 'Routines', slot };
		}),
		state: p.watched ? 'watched' : 'saved',
		visits: p.visits,
		...at(p)
	}));
}

/** Map search hits as result rows, keyed by index, with a distance from `here` when known. */
export function resultRows(hits: PlaceHit[], here: { lat: number; lon: number } | null): PlaceResult[] {
	const at = grid(hits);
	return hits.map((h, i) => {
		const km = here ? Math.hypot((h.lat - here.lat) * 111, (h.lon - here.lon) * 111 * Math.cos((here.lat * Math.PI) / 180)) : null;
		return { id: String(i), name: h.name ?? h.address ?? 'Place', address: h.address ?? '', distance: km === null ? '' : `${km.toFixed(1)} km`, ...at(h) };
	});
}

/** Routines dashboard rows: the streak label with its unit, last done and the next occurrence's tone. */
export function routineRows(v: LocationView, now: string, zone: string): RoutineRow[] {
	const today = formatYmd(now, zone);
	return v.routines.map((r) => {
		const next = r.next;
		const [tone, due]: [RoutineRow['tone'], string] = !next
			? ['skipped', 'Not planned']
			: next.status === 'missed'
				? ['missed', 'Missed']
				: formatYmd(next.start_at, zone) === today
					? ['warn', 'Due today']
					: ['planned', 'Planned'];
		return { id: r.series_id, title: r.title, place: r.place, cadence: r.cadence, streak: r.streak, lastDone: r.last_done, nextDue: next?.start_at ?? null, tone, due };
	});
}

/** The location log rows. */
export function visitRows(v: LocationView): VisitRow[] {
	return v.log.map((l) => ({ id: l.id, place: l.place, arrived: l.arrived_at, left: l.left_at }));
}

/** Settings > Location: on means Always, which the enable flow asks for. */
export function locationSettingsView(v: LocationView): LocationSettingsView {
	const s = v.settings;
	return {
		permission: s.enabled ? 'always' : 'off',
		paused: s.paused,
		nudges: s.nudges,
		watched: s.watched,
		visits: s.visits,
		ignored: s.ignored.map((i) => ({ id: i.id, name: i.name, address: i.address ?? '' }))
	};
}
