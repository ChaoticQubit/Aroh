import { catalog } from '$lib/app/catalog';
import type { ActionView, CategorySlot, ChatMessage, ComponentSpec, Point, Ring, SpecOf, ViewSpec } from '../contract';
import type { GoalHeader, GoalRow, InboxItem, ProviderView, SearchView, SettingsView } from '../screens/types';

/** Zone every mockup formats times in. Sample instants are written in this zone. */
export const mockZone = 'UTC';

/** The instant the mockups treat as now: Thursday 26 Nov 2026, 08:15. */
export const mockNow = '2026-11-26T08:15:00Z';

/** Seeded goal categories with their colour slot. */
export const categories: { name: string; slot: CategorySlot }[] = [
	{ name: 'Learning', slot: 1 },
	{ name: 'Health and Fitness', slot: 2 },
	{ name: 'Financial', slot: 3 },
	{ name: 'Hobby and Skill', slot: 4 },
	{ name: 'Career and Projects', slot: 5 },
	{ name: 'Habit', slot: 6 }
];

/** The sample goal every mockup is built around. */
export const sixpackGoal = {
	id: 'g-sixpack',
	title: 'Six pack by Feb 1',
	category: 'Health and Fitness',
	slot: 2 as CategorySlot,
	deadline: '2027-02-01T00:00:00Z',
	autonomy: 'Balanced' as 'Ask' | 'Balanced' | 'Auto',
	personas: ['Fitness coach', 'Nutritionist', 'Planner'],
	trackers: [
		{ name: 'Weight', unit: 'kg' as const, now: 82.4, target: 78 },
		{ name: 'Body fat', unit: 'count' as const, now: 18, target: 12 },
		{ name: 'Protein', unit: 'g' as const, now: 141, target: 150 },
		{ name: 'Workouts', unit: 'count' as const, now: 3, target: 4 }
	]
};

const FIRST_DAY = Date.UTC(2026, 10, 21);
const DAY_MS = 86_400_000;
const DAYS = 14;
const MISSED_ID = 'a-2026-11-25-1800-2';
const SIX = 'Six pack by Feb 1';

/**
 * Builds one action on sample day `day` (0 is Sat 21 Nov) from "HH:MM" start and end.
 * Status is derived from `mockNow`: past actions are done unless listed as missed.
 * Returns a complete ActionView.
 */
function action(day: number, start: string, end: string, title: string, slot: CategorySlot, goalTitle: string, locked = false): ActionView {
	const date = new Date(FIRST_DAY + day * DAY_MS).toISOString().slice(0, 10);
	const startIso = `${date}T${start}:00Z`;
	const endIso = `${date}T${end}:00Z`;
	const id = `a-${date}-${start.replace(':', '')}-${slot}`;
	const past = Date.parse(endIso) <= Date.parse(mockNow);
	const missed = id === MISSED_ID;
	return {
		id,
		title,
		start: startIso,
		end: endIso,
		status: missed ? 'missed' : past ? 'done' : 'planned',
		locked,
		category: slot,
		goalTitle
	};
}

/** Fourteen days of actions from Sat 21 Nov: the goal's sessions plus busy blocks from two other goals. */
export const sixpackActions: ActionView[] = Array.from({ length: DAYS }, (_, day) => {
	const weekday = new Date(FIRST_DAY + day * DAY_MS).getUTCDay();
	const list: ActionView[] = [];
	if (weekday === 2 || weekday === 4) list.push(action(day, '07:00', '07:30', 'Core session', 2, SIX));
	if (weekday === 0) list.push(action(day, '10:00', '10:30', 'Meal prep', 2, SIX));
	if (weekday === 5) list.push(action(day, '12:00', '12:30', 'Weekly budget review', 3, 'Save $10k', true));
	if (weekday === 1 || weekday === 3 || weekday === 5) list.push(action(day, '18:00', '19:00', 'Strength session', 2, SIX));
	list.push(action(day, '20:00', '20:30', 'Piano practice', 4, 'Learn piano'));
	return list;
}).flat();

const WEIGHTS = [84.0, 83.8, 83.9, 83.6, 83.7, 83.4, 83.5, 83.3, 83.4, 83.1, 83.2, 83.0, 82.9, 83.1, 82.8, 82.9, 82.7, 82.6, 82.8, 82.5, 82.4];
const PROTEIN = [138, 152, 118, 145, 162, 131, 155];

/** Builds a daily point series ending on the `mockNow` date. Returns one point per value. */
function daily(values: number[]): Point[] {
	const last = Date.parse(mockNow.slice(0, 10) + 'T07:00:00Z');
	return values.map((value, i) => ({ at: new Date(last - (values.length - 1 - i) * DAY_MS).toISOString(), value }));
}

/** Tracker history: 21 days of weight, 7 days of protein and recent meal log entries. */
export const sixpackEntries = {
	weight: daily(WEIGHTS),
	protein: daily(PROTEIN),
	meals: [
		{ at: '2026-11-26T07:40:00Z', text: 'Greek yogurt, oats and berries', value: 28, unit: 'g' as const },
		{ at: '2026-11-25T19:30:00Z', text: 'Salmon, potatoes and greens', value: 46, unit: 'g' as const },
		{ at: '2026-11-25T13:00:00Z', text: 'Chicken rice bowl', value: 42, unit: 'g' as const },
		{ at: '2026-11-25T08:00:00Z', text: 'Three eggs on toast', value: 24, unit: 'g' as const }
	]
};

/** Streak data for the last 28 days: 0 nothing done, 3 everything done. */
export const sixpackStreak: { date: string; level: 0 | 1 | 2 | 3 }[] = [
	2, 3, 0, 3, 2, 1, 3, 3, 2, 0, 3, 3, 1, 2, 3, 3, 0, 2, 3, 2, 1, 3, 3, 2, 3, 0, 3, 2
].map((level, i) => ({
	date: new Date(Date.parse('2026-11-26T00:00:00Z') - (27 - i) * DAY_MS).toISOString().slice(0, 10),
	level: level as 0 | 1 | 2 | 3
}));

/** The check-in question the Fitness coach asks this morning. */
export const weightCheckIn: ComponentSpec = {
	kind: 'question_card',
	persona: 'Fitness coach',
	question: "What is today's weight?",
	input: { kind: 'number_input', label: 'Weight', unit: 'kg', value: null, step: 0.1, min: 30, max: 250 }
};

/** Inbox content, grouped the way the Inbox screen lists it. */
export const sixpackInbox = {
	approvals: [
		{
			kind: 'proposal_card',
			persona: 'Fitness coach',
			summary: "Move Wednesday's missed strength session to Thursday 18:00 and shorten Thursday core to 20 min.",
			changes: [
				{ op: 'move', title: 'Strength session', before: 'Wed 25 Nov 18:00', after: 'Thu 26 Nov 18:00' },
				{ op: 'resize', title: 'Core session', before: 'Thu 07:00 to 07:30', after: 'Thu 07:00 to 07:20' }
			]
		}
	] satisfies ComponentSpec[],
	suggestions: [
		{
			kind: 'proposal_card',
			persona: 'Planner',
			summary: 'You moved workouts to evenings 3 times. Plan them 18:00 to 20:00 from now on?',
			changes: []
		}
	] satisfies ComponentSpec[],
	insights: [
		{
			kind: 'insight_card',
			persona: 'Nutritionist',
			title: 'Protein below 150 g on 4 of the last 7 days',
			body: 'Add one high-protein snack on training days, for example 250 g of skyr or a shake after the session.'
		}
	] satisfies ComponentSpec[],
	questions: [weightCheckIn],
	runs: [
		{ persona: 'Fitness coach', title: 'Weekly review of training load', step: 'Reading the last 7 days of sessions', progress: 60 },
		{ persona: 'Nutritionist', title: 'Meal plan for next week', step: 'Waiting for the weekly review', progress: 10 }
	]
};

/** The New Goal conversation, from the first message to the plan awaiting approval. */
export const sixpackChat: ChatMessage[] = [
	{ type: 'message', from: 'owner', name: 'You', text: 'I want a six pack by Feb 1' },
	{
		type: 'message',
		from: 'persona',
		name: 'Orchestrator',
		text: 'Good goal, about 10 weeks out. I brought in a Fitness coach, a Nutritionist and a Planner. A few questions first.'
	},
	{
		type: 'card',
		spec: {
			kind: 'question_card',
			persona: 'Fitness coach',
			question: 'What do you weigh today?',
			input: { kind: 'number_input', label: 'Weight', unit: 'kg', value: 82.4, step: 0.1, min: 30, max: 250 }
		}
	},
	{
		type: 'card',
		spec: {
			kind: 'question_card',
			persona: 'Nutritionist',
			question: 'How would you describe your current diet?',
			input: { kind: 'choice', label: 'Diet', options: ['No plan', 'Count calories', 'High protein', 'Vegetarian'], multi: false, value: ['No plan'] }
		}
	},
	{
		type: 'card',
		spec: {
			kind: 'question_card',
			persona: 'Planner',
			question: 'Which days can you train?',
			input: { kind: 'choice', label: 'Training days', options: ['Mon', 'Tue', 'Wed', 'Thu', 'Fri', 'Sat', 'Sun'], multi: true, value: ['Mon', 'Tue', 'Wed', 'Thu', 'Fri'] }
		}
	},
	{
		type: 'progress',
		lines: [
			{ text: 'Reading 3 sources on fat loss rates', done: true },
			{ text: 'Nutritionist setting a protein and calorie target', done: true },
			{ text: 'Fitness coach drafting a 10-week split', done: false }
		]
	},
	{
		type: 'plan',
		title: 'Plan for Six pack by Feb 1',
		items: [
			'Strength Mon, Wed, Fri 18:00 to 19:00',
			'Core Tue, Thu 07:00 to 07:30',
			'Meal prep Sundays 10:00',
			'Targets: 78 kg, 150 g protein a day, 4 workouts a week',
			'Check-ins: weight every morning, meals as you eat'
		]
	}
];

/** The goal dashboard as a plugin view tree. */
/** The goal's top three trackers this week, one ring each: workouts, protein days and weigh-ins. */
export const weekRings: Ring[] = [
	{ label: 'Workouts', unit: 'count', value: 3, target: 4, series: 1 },
	{ label: 'Protein days', unit: 'count', value: 3, target: 7, series: 2 },
	{ label: 'Weigh-ins', unit: 'count', value: 6, target: 7, series: 3 }
];

export const sixpackView: ViewSpec = {
	title: 'Six pack by Feb 1',
	sections: [
		{
			title: 'Progress',
			items: [
				{ kind: 'countdown', label: 'Until Feb 1', deadline: sixpackGoal.deadline },
				{ kind: 'progress_ring', label: 'This week', rings: weekRings },
				{ kind: 'line_chart', label: 'Weight', unit: 'kg', points: sixpackEntries.weight, target: 78, series: 3 },
				{ kind: 'bar_chart', label: 'Protein per day', unit: 'g', points: sixpackEntries.protein, target: 150, series: 2 },
				{ kind: 'streak_heatmap', label: 'Last 4 weeks', days: sixpackStreak, series: 2 }
			]
		},
		{
			title: 'Upcoming',
			items: [
				{
					kind: 'action_list',
					label: 'Next sessions',
					actions: sixpackActions.filter((a) => a.goalTitle === SIX && a.status === 'planned').slice(0, 4)
				}
			]
		}
	]
};

/** Model providers the owner can pick in Settings, with sample key flags. */
export const providerCatalog: ProviderView[] = catalog.map((p) => ({ ...p, keySet: p.id === 'anthropic' || p.id === 'ollama', last4: p.id === 'anthropic' ? '7Q2x' : null }));

/** Web search providers configured in Settings, in fallback order. */
export const searchProviders: SearchView[] = [
	{ name: 'Brave Search', detail: 'API key', keySet: true },
	{ name: 'SearXNG', detail: 'https://search.home.lan', keySet: true }
];

/** The Settings screen's sample values. */
export const sixpackSettings: SettingsView = {
	providers: providerCatalog,
	providerId: 'anthropic',
	model: 'claude-opus-5-5',
	search: searchProviders,
	serverUrl: 'https://aroh-engine.example.com',
	budgetAll: 20,
	budgetGoal: 8,
	calendars: true,
	health: false,
	account: { supabaseUrl: 'https://aroh.supabase.co', anonKey: 'eyJhbGciOiJIUzI1NiJ9.sample', email: 'sam@example.com' }
};

/** The Goal screen's header facts for the sample goal. */
export const sixpackHeader: GoalHeader = {
	id: sixpackGoal.id,
	title: sixpackGoal.title,
	category: sixpackGoal.category,
	slot: sixpackGoal.slot,
	deadline: sixpackGoal.deadline,
	autonomy: sixpackGoal.autonomy,
	personas: sixpackGoal.personas,
	lead: { unit: 'kg', now: sixpackGoal.trackers[0].now, target: sixpackGoal.trackers[0].target }
};

/** The Goals list: the sample goal plus the two goals its busy blocks come from. */
export const sixpackGoals: GoalRow[] = [
	{ id: sixpackGoal.id, title: sixpackGoal.title, category: sixpackGoal.category, slot: sixpackGoal.slot, deadline: sixpackGoal.deadline, autonomy: sixpackGoal.autonomy },
	{ id: 'g-piano', title: 'Learn piano', category: 'Hobby and Skill', slot: 4, deadline: null, autonomy: 'Ask' },
	{ id: 'g-save', title: 'Save $10k', category: 'Financial', slot: 3, deadline: '2027-06-30T00:00:00Z', autonomy: 'Balanced' }
];

/** Inbox cards keyed by a sample row id, in the groups the Inbox screen lists. */
export const sixpackInboxItems: { approvals: InboxItem[]; suggestions: InboxItem[]; insights: InboxItem[] } = {
	approvals: sixpackInbox.approvals.map((spec, i) => ({ id: `p-${i}`, spec })),
	suggestions: sixpackInbox.suggestions.map((spec, i) => ({ id: `s-${i}`, spec })),
	insights: sixpackInbox.insights.map((spec, i) => ({ id: `i-${i}`, spec }))
};

/** The Log screen's sample: the weight check-in, one text waiting for the engine and recent meals. */
export const sixpackLog: { checkIns: { trackerId: string; spec: ComponentSpec }[]; waiting: SpecOf<'log_list'>; recent: SpecOf<'log_list'> } = {
	checkIns: [{ trackerId: 't-weight', spec: weightCheckIn }],
	waiting: { kind: 'log_list', label: 'Waiting for the engine', entries: [{ at: '2026-11-26T08:05:00Z', text: 'Coffee with oat milk and a banana' }] },
	recent: { kind: 'log_list', label: 'Recent entries', entries: sixpackEntries.meals }
};
