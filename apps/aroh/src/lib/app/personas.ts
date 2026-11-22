import type { CategoryView, CoachGroup, CoachView } from '$lib/ui/screens/types';
import type { Persona as PersonaRow } from './client';

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

/**
 * The coaches that worked on goal `goalId`: its persona rows and the persona slugs of its runs,
 * once each, in library order, then the slugs the library lacks, which show under their slug.
 * `runs` are the goal's runs as `GET /v1/runs?goal_id=` lists them.
 */
export function coachesOn(views: PersonaView[], rows: PersonaRow[], runs: { personas?: string[] }[], goalId: string): CoachView[] {
	const names = new Set([...rows.filter((r) => r.goal_id === goalId).map((r) => r.name), ...runs.flatMap((r) => r.personas ?? [])]);
	const known = views.filter((v) => names.has(v.name));
	const unknown = [...names].filter((n) => !views.some((v) => v.name === n));
	return [...known, ...unknown.map((name): CoachView => ({ name, display_name: name, role: '', instructions: '', guardrails: [], locked: [], source: 'builtin' }))];
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
