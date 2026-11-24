import type { CategoryView, CoachGroup, CoachView, PluginUsed } from '$lib/ui/screens/types';
import type { Persona as PersonaRow, Snapshot } from './client';
import { engine, type EngineResult } from './engine';

/** The kind of goal a team serves, as the engine lists it on a persona. */
export type TeamKind = 'general' | 'fitness' | 'learning' | 'music' | 'finance' | 'habits';

/** One persona of `GET /v1/personas`: its text fields, locked safety rules, teams and origin. */
export type PersonaView = CoachView & { teams: TeamKind[]; builtin: boolean; toml: string };

/** One team group of the library: its persona kind, its title and the seeded category it colours by. */
type Team = { kind: TeamKind; title: string; category: string };

const TEAMS: readonly Team[] = [
	{ kind: 'fitness', title: 'Fitness', category: 'Health and Fitness' },
	{ kind: 'music', title: 'Music', category: 'Hobby and Skill' },
	{ kind: 'learning', title: 'Learning', category: 'Learning' },
	{ kind: 'finance', title: 'Finance', category: 'Financial' },
	{ kind: 'habits', title: 'Habits', category: 'Habit' }
];
const SAFETY_ERROR = /: guardrails: keep `(.+)`$/s;
const INVALID = /^(setup: )?invalid_persona: /;

/**
 * Groups personas by team in the order fitness, music, learning, finance, habits, then "Shared"
 * for personas in none of them. A persona in several teams shows in each. A group's dot is the
 * slot of its seeded category in `categories`, null when absent; empty groups are left out.
 */
export function groupByTeam(views: PersonaView[], categories: CategoryView[]): (CoachGroup & { personas: PersonaView[] })[] {
	const groups = TEAMS.map((t) => ({ title: t.title, slot: categories.find((c) => c.name === t.category)?.slot ?? null, personas: views.filter((v) => v.teams.includes(t.kind)) }));
	const shared = views.filter((v) => !TEAMS.some((t) => v.teams.includes(t.kind)));
	return [...groups, { title: 'Shared', slot: null, personas: shared }].filter((g) => g.personas.length > 0);
}

/** The run fields the goal page reads: id, persona slugs, and the plugin version the run used. */
export type GoalRun = { run_id?: string; personas?: string[]; plugin_id?: string | null; plugin_version?: number | null };

/**
 * The coaches that worked on goal `goalId`: its persona rows and the persona slugs of its runs,
 * once each, in library order, then the slugs the library lacks, which show under their slug.
 * `runs` counts the goal's runs naming the coach; `fresh` is true when an agent created its row
 * in one of them. `runs` are the goal's runs as `GET /v1/runs?goal_id=` lists them.
 */
export function coachesOn(views: PersonaView[], rows: PersonaRow[], runs: GoalRun[], goalId: string): CoachView[] {
	const mine = rows.filter((r) => r.goal_id === goalId);
	const names = new Set([...mine.map((r) => r.name), ...runs.flatMap((r) => r.personas ?? [])]);
	const ids = new Set(runs.map((r) => r.run_id).filter(Boolean));
	const known = views.filter((v) => names.has(v.name));
	const unknown = [...names].filter((n) => !views.some((v) => v.name === n));
	const all = [...known, ...unknown.map((name): CoachView => ({ name, display_name: name, role: '', instructions: '', guardrails: [], locked: [], source: 'builtin' }))];
	return all.map((c) => ({
		...c,
		runs: runs.filter((r) => r.personas?.includes(c.name)).length,
		fresh: mine.some((r) => r.name === c.name && !!r.created_run_id && ids.has(r.created_run_id))
	}));
}

/**
 * The plugins goal runs used, one per plugin name: the newest version a run used, how many runs
 * used any of its versions, and `fresh` when a run of this goal created one of those versions.
 * `plugins` are the goal's plugin versions; `runs` the goal's runs.
 */
export function pluginsUsed(plugins: { id: string; name: string; version: number; created_run_id?: string | null }[], runs: GoalRun[]): PluginUsed[] {
	const ids = new Set(runs.map((r) => r.run_id).filter(Boolean));
	const byName = new Map<string, PluginUsed>();
	for (const p of plugins) {
		const used = runs.filter((r) => r.plugin_id === p.id);
		if (!used.length) continue;
		const at = byName.get(p.name) ?? { name: p.name, version: 0, runs: 0, fresh: false };
		byName.set(p.name, { name: p.name, version: Math.max(at.version, ...used.map((r) => r.plugin_version ?? p.version)), runs: at.runs + used.length, fresh: at.fresh || (!!p.created_run_id && ids.has(p.created_run_id)) });
	}
	return [...byName.values()];
}

/** The engine reads `loadCoaches` makes, injectable for tests. */
export type CoachDeps = {
	personas: () => Promise<EngineResult<PersonaView[]>>;
	runs: (filter: { goalId?: string }) => Promise<EngineResult<GoalRun[]>>;
};

/**
 * The persona library and, for goal `goalId`, the coaches that worked on it (`coachesOn` over the
 * snapshot's persona rows and the goal's runs). Offline lists the rows only, with the engine's
 * message as `status`; no goal gives no coaches. `runs` are the goal's runs, for `pluginsUsed`.
 */
export async function loadCoaches(s: Snapshot, goalId: string | null, deps: CoachDeps = engine): Promise<{ views: PersonaView[]; coaches: CoachView[]; runs: GoalRun[]; status: string | null }> {
	const [library, runs] = await Promise.all([deps.personas(), goalId ? deps.runs({ goalId }) : null]);
	const views = library.ok ? library.value : [];
	const goalRuns = runs?.ok ? runs.value : [];
	const coaches = goalId ? coachesOn(views, s.personas, goalRuns, goalId) : [];
	return { views, coaches, runs: goalRuns, status: library.ok ? null : library.message };
}

/**
 * The owner-facing text of a failed persona edit: a dropped safety rule as "Safety rules stay on
 * every coach: <rule>", any other invalid persona without its code prefix, the rest unchanged.
 */
export function editError(message: string): string {
	const safety = INVALID.test(message) ? SAFETY_ERROR.exec(message) : null;
	if (safety) return `Safety rules stay on every coach: ${safety[1]}`;
	return message.replace(INVALID, '');
}
