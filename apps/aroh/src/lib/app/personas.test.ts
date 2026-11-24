import { describe, expect, it } from 'vitest';
import { coachesOn, editError, groupByTeam, loadCoaches, pluginsUsed, type PersonaView } from './personas';
import type { Snapshot } from './client';

const SAFETY = 'Safety: no individual investment, tax or legal advice; refer to a licensed financial professional for those.';

/** A listed persona called `name` in `teams`, a built-in unless `source` says otherwise. */
function persona(name: string, teams: PersonaView['teams'], extra: Partial<PersonaView> = {}): PersonaView {
	return { name, display_name: name.replace('-', ' '), role: `${name} role`, instructions: 'Do it.', guardrails: [], locked: [], teams, source: 'builtin', builtin: true, toml: '', ...extra };
}

const views = [
	persona('goal-researcher', ['general', 'fitness', 'habits']),
	persona('goal-planner', ['general', 'fitness']),
	persona('tutor', ['learning']),
	persona('practice-coach', ['music'], { source: 'library' }),
	persona('budget-coach', ['finance'], { guardrails: ['Cite sources.', SAFETY], locked: [SAFETY] }),
	persona('habit-coach', ['habits']),
	persona('plugin-builder', [])
];
const categories = [
	{ name: 'Learning', slot: 1 as const },
	{ name: 'Health and Fitness', slot: 2 as const },
	{ name: 'Financial', slot: 3 as const },
	{ name: 'Hobby and Skill', slot: 4 as const },
	{ name: 'Habit', slot: 6 as const }
];

describe('persona library', () => {
	it('groups coaches by team in a fixed order, then Shared', () => {
		const groups = groupByTeam(views, categories);
		expect(groups.map((g) => [g.title, g.slot, g.personas.map((p) => p.name)])).toEqual([
			['Fitness', 2, ['goal-researcher', 'goal-planner']],
			['Music', 4, ['practice-coach']],
			['Learning', 1, ['tutor']],
			['Finance', 3, ['budget-coach']],
			['Habits', 6, ['goal-researcher', 'habit-coach']],
			['Shared', null, ['plugin-builder']]
		]);
		expect(groupByTeam(views, []).every((g) => g.slot === null)).toBe(true);
		expect(groupByTeam([persona('tutor', ['learning'])], categories).map((g) => g.title)).toEqual(['Learning']);
	});

	it('lists the coaches of one goal from its persona rows and its runs', () => {
		const rows = [
			{ id: '1', goal_id: 'g1', name: 'practice-coach' },
			{ id: '3', goal_id: 'g2', name: 'tutor' },
			{ id: '4', goal_id: null, name: 'budget-coach' },
			{ id: '6', goal_id: 'g1', name: 'retired-coach' }
		];
		const runs = [{ personas: ['goal-planner', 'goal-researcher', 'intake-interviewer'] }, { personas: ['goal-planner', 'practice-coach'] }];
		expect(coachesOn(views, rows, runs, 'g1').map((c) => [c.name, c.display_name])).toEqual([
			['goal-researcher', 'goal researcher'],
			['goal-planner', 'goal planner'],
			['practice-coach', 'practice coach'],
			['retired-coach', 'retired-coach'],
			['intake-interviewer', 'intake-interviewer']
		]);
		expect(coachesOn(views, rows, [], 'none')).toEqual([]);
	});

	it('counts runs per coach and marks coaches and plugins an agent made in this goal as new (Q72)', () => {
		const rows = [
			{ id: '1', goal_id: 'g1', name: 'practice-coach', created_run_id: 'r2' },
			{ id: '2', goal_id: 'g1', name: 'old-coach', created_run_id: 'other-goal-run' }
		];
		const runs = [
			{ run_id: 'r1', personas: ['goal-planner'], plugin_id: 'p1', plugin_version: 1 },
			{ run_id: 'r2', personas: ['goal-planner', 'practice-coach'], plugin_id: 'p2', plugin_version: 2 }
		];
		const coaches = coachesOn(views, rows, runs, 'g1');
		expect(coaches.map((c) => [c.name, c.runs, c.fresh])).toEqual([
			['goal-planner', 2, false],
			['practice-coach', 1, true],
			['old-coach', 0, false]
		]);
		const plugins = [
			{ id: 'p2', name: 'Reading dashboard', version: 2, created_run_id: 'r2' },
			{ id: 'p1', name: 'Reading dashboard', version: 1, created_run_id: null }
		];
		expect(pluginsUsed(plugins, runs)).toEqual([{ name: 'Reading dashboard', version: 2, runs: 2, fresh: true }]);
		expect(pluginsUsed([], [])).toEqual([]);
	});

	it('loads the library and the coaches of one goal, empty when offline', async () => {
		const s = { personas: [{ id: '1', goal_id: 'g1', name: 'practice-coach' }] } as unknown as Snapshot;
		const asked: (string | undefined)[] = [];
		const online = {
			personas: async () => ({ ok: true as const, value: views }),
			runs: async (q: { goalId?: string }) => (asked.push(q.goalId), { ok: true as const, value: [{ personas: ['goal-planner'] }] })
		};
		const loaded = await loadCoaches(s, 'g1', online);
		expect(loaded.coaches.map((c) => c.name)).toEqual(['goal-planner', 'practice-coach']);
		expect([loaded.views.length, loaded.status]).toEqual([views.length, null]);
		expect(asked).toEqual(['g1']);
		const library = await loadCoaches(s, null, online);
		expect([library.coaches, library.views.length]).toEqual([[], views.length]);
		expect(asked, 'no goal, no runs read').toEqual(['g1']);
		const offline = { personas: async () => ({ ok: false as const, reason: 'needs_engine' as const, message: 'Offline.' }), runs: async () => ({ ok: false as const, reason: 'needs_engine' as const, message: 'Offline.' }) };
		const away = await loadCoaches(s, 'g1', offline);
		expect([away.views, away.coaches.map((c) => c.name), away.status]).toEqual([[], ['practice-coach'], 'Offline.']);
	});

	it('turns engine errors into owner text', () => {
		expect(editError(`setup: invalid_persona: budget-coach.toml: guardrails: keep \`${SAFETY}\``)).toBe(`Safety rules stay on every coach: ${SAFETY}`);
		expect(editError('setup: invalid_persona: budget-coach.toml: display_name: must not be blank')).toBe('budget-coach.toml: display_name: must not be blank');
		expect(editError('invalid_persona: tutor.toml: role: must not be blank')).toBe('tutor.toml: role: must not be blank');
		expect(editError('The engine answered 500.')).toBe('The engine answered 500.');
	});
});
