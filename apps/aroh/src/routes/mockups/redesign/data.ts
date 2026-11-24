import type { CategorySlot } from '$lib/ui/contract';

/** One step of an agent run with what it produced; `coach` names the persona that ran it. */
export type RunStep = { step: string; coach: string; output: string; seconds: number; tools: string[] };

/** One agent run on a goal: what started it, its steps, its cost and how it ended. */
export type RunLog = { title: string; when: string; cost: string; state: 'done' | 'waiting' | 'stopped'; steps: RunStep[] };

/** A coach or plugin that worked on a goal; `fresh` marks one created during the goal's runs. */
export type Helper = { kind: 'coach' | 'plugin'; name: string; role: string; runs: number; fresh: boolean; version?: number };

/** One question the agents asked with the owner's answer, null when skipped or still open. */
export type QA = { q: string; a: string | null; unit?: string };

/** One research source with the site it came from and what it was used for. */
export type Source = { title: string; site: string; note: string };

/** One upcoming session of the goal. */
export type Session = { day: string; time: string; title: string; minutes: number };

/** Everything the goal-page mockup shows for one goal. */
export type GoalSeed = {
	title: string;
	category: string;
	slot: CategorySlot;
	deadline: string;
	status: { tone: 'done' | 'planned' | 'warn' | 'missed' | 'skipped'; chip: string; line: string };
	progress: { done: number; total: number; unit: string; finish: string; pace: string };
	next: Session[];
	waiting: boolean;
	answers: QA[];
	research: { summary: string; sources: Source[]; files: string[] };
	runs: RunLog[];
	helpers: Helper[];
	chat: { who: string; text: string }[];
};

const step = (stepName: string, coach: string, output: string, seconds: number, tools: string[] = []): RunStep => ({ step: stepName, coach, output, seconds, tools });

/** A goal with an approved plan: the default open and closed state of every card. */
export const readingGoal: GoalSeed = {
	title: 'Finish reading Harbor Lights',
	category: 'Learning',
	slot: 1,
	deadline: '31 Dec',
	status: { tone: 'done', chip: 'On track', line: 'Agents are idle. Next check after Sunday’s session.' },
	progress: { done: 9, total: 24, unit: 'chapters', finish: 'Finishes 14 Dec', pace: '17 days before your date' },
	next: [
		{ day: 'Today', time: '18:00', title: 'Read chapter 10', minutes: 20 },
		{ day: 'Fri 27', time: '18:00', title: 'Read chapter 11', minutes: 20 },
		{ day: 'Sat 28', time: '20:30', title: 'Read chapter 12', minutes: 20 }
	],
	waiting: false,
	answers: [
		{ q: 'Which chapter are you on?', a: 'Chapter 6' },
		{ q: 'How many minutes a day can you read?', a: '20', unit: 'min' },
		{ q: 'When do you like to read?', a: 'Evenings' }
	],
	research: {
		summary: 'Harbor Lights has 24 chapters of about 14 pages. At your pace a chapter takes about 20 minutes.',
		sources: [
			{ title: 'Harbor Lights, publisher page', site: 'penguin.example', note: 'Chapter count and page length' },
			{ title: 'Reading speed for literary fiction', site: 'readingresearch.example', note: 'Minutes per page' }
		],
		files: []
	},
	runs: [
		{
			title: 'New goal plan',
			when: 'Mon 23 Nov, 09:12',
			cost: '$0.04',
			state: 'done',
			steps: [
				step('Research', 'Tutor', 'Found 24 chapters, about 14 pages each.', 21, ['web_search', 'web_fetch']),
				step('Questions', 'Tutor', 'Asked 3 questions, all answered.', 2),
				step('Plan', 'Practice planner', '19 sessions of 20 min at 18:00, 3 moved around busy evenings.', 14, ['read_calendar', 'write_plan']),
				step('Plugin', 'Plugin builder', 'Built the "Reading log" plugin, version 1.', 9, ['store_plugin'])
			]
		}
	],
	helpers: [
		{ kind: 'coach', name: 'Tutor', role: 'Researches the book and asks what it cannot find', runs: 1, fresh: false },
		{ kind: 'coach', name: 'Practice planner', role: 'Turns the answers into sessions', runs: 1, fresh: false },
		{ kind: 'plugin', name: 'Reading log', role: 'Chapters read, minutes per session', runs: 1, fresh: true, version: 1 }
	],
	chat: [
		{ who: 'Tutor', text: 'Plan approved. I will check in after Sunday.' },
		{ who: 'You', text: 'Can I skip Saturday?' },
		{ who: 'Practice planner', text: 'Yes. Saturday moves to Sunday 18:00, the finish date stays 15 Dec.' }
	]
};

const longSources: Source[] = [
	['Couch to half marathon in 16 weeks', 'runnersworld.example', 'Weekly long-run progression'],
	['Heart rate zones for beginners', 'nhs.example', 'Easy pace at zone 2'],
	['Ten percent rule, revisited', 'sportsmed.example', 'Weekly mileage cap'],
	['Strength work for runners', 'physio.example', 'Two short sessions a week'],
	['Taper before a half marathon', 'runnersworld.example', 'Last two weeks'],
	['Rest days and injury risk', 'bjsm.example', 'At least one full rest day'],
	['Fuelling long runs', 'nutrition.example', 'Gels after 60 minutes'],
	['Running in winter', 'weather.example', 'Daylight and ice'],
	['Shoe mileage', 'gear.example', 'Replace after 600 km'],
	['Lisbon half marathon route', 'maratonalisboa.example', 'Race date 14 Mar, hilly last 5 km'],
	['Cadence drills', 'coaching.example', 'Strides twice a week'],
	['Sleep and recovery', 'sleep.example', 'Seven hours on long-run nights']
].map(([title, site, note]) => ({ title, site, note }));

/** A long goal for the long-data frames: long title, waiting questions, many runs, sources and helpers. */
export const halfGoal: GoalSeed = {
	title: 'Run the Lisbon half marathon in under two hours without getting injured',
	category: 'Health and fitness',
	slot: 2,
	deadline: '14 Mar',
	status: { tone: 'warn', chip: 'Waiting for you', line: '2 questions about your long-run day. The weekly review waits for them.' },
	progress: { done: 31, total: 106, unit: 'sessions', finish: 'Finishes 9 Mar', pace: '5 days before race day, 2 sessions behind this week' },
	next: [
		{ day: 'Today', time: '07:00', title: 'Easy run, 6 km at zone 2', minutes: 40 },
		{ day: 'Fri 27', time: '18:30', title: 'Strength for runners: squats, lunges, calf raises', minutes: 30 },
		{ day: 'Sat 28', time: '08:00', title: 'Long run, 14 km with the last 2 km at race pace', minutes: 85 },
		{ day: 'Mon 30', time: '07:00', title: 'Strides and cadence drills', minutes: 35 },
		{ day: 'Tue 1', time: '07:00', title: 'Tempo run, 8 km', minutes: 50 }
	],
	waiting: true,
	answers: [
		{ q: 'Which day suits your long run best now that Saturdays are busy?', a: null },
		{ q: 'How long can the long run be on that day?', a: null, unit: 'min' },
		{ q: 'What is your current 5 km time?', a: '27:40' },
		{ q: 'How many days a week can you run?', a: '4' },
		{ q: 'Any injuries in the last year?', a: 'Left knee, mild, gone since June' },
		{ q: 'Do you have a gym nearby?', a: 'Yes, 10 minutes walk' }
	],
	research: {
		summary: 'A 16-week plan from 4 runs a week fits: easy runs at zone 2, one long run growing by 10% a week with a cut-back every fourth week, two short strength sessions and a two-week taper before 14 Mar.',
		sources: longSources,
		files: ['Training log 2025.pdf', 'Knee physio notes.jpg', 'strava.com/athletes/me']
	},
	runs: [
		{
			title: 'Weekly review',
			when: 'Sun 22 Nov, 20:00',
			cost: '$0.06',
			state: 'waiting',
			steps: [
				step('Read logs', 'Running coach', '4 of 4 runs done, long run 12 km at 6:05 per km.', 4, ['read_trackers']),
				step('Check calendar', 'Running coach', 'Saturdays are busy for the next 3 weeks.', 3, ['read_calendar']),
				step('Ask', 'Running coach', 'Asked 2 questions about the long-run day.', 2)
			]
		},
		{
			title: 'Missed session replan',
			when: 'Thu 19 Nov, 21:40',
			cost: '$0.03',
			state: 'done',
			steps: [
				step('Notice', 'Habit coach', 'Tempo run on Thursday was missed.', 1),
				step('Replan', 'Running coach', 'Moved the tempo run to Friday 07:00, applied with undo.', 6, ['propose_replan'])
			]
		},
		{
			title: 'New goal plan',
			when: 'Mon 9 Nov, 08:30',
			cost: '$0.21',
			state: 'done',
			steps: [
				step('Research', 'Running coach', 'Read 12 sources: progression, zones, taper, the race route.', 48, ['web_search', 'web_fetch', 'web_fetch', 'web_fetch']),
				step('Read files', 'Running coach', 'Read your 2025 training log and physio notes.', 12, ['read_file']),
				step('Questions', 'Running coach', 'Asked 4 questions, all answered.', 2),
				step('Plan', 'Endurance planner', '96 sessions over 16 weeks, long runs on Saturday.', 31, ['read_calendar', 'write_plan']),
				step('Safety', 'Injury check', 'Kept weekly growth at 10% because of the knee note.', 8),
				step('Plugin', 'Plugin builder', 'Built "Run log" with pace and distance charts.', 17, ['store_plugin']),
				step('Coach', 'Plugin builder', 'Created the "Strength for runners" coach for the gym days.', 11, ['store_plugin'])
			]
		}
	],
	helpers: [
		{ kind: 'coach', name: 'Running coach', role: 'Research, questions and weekly reviews', runs: 3, fresh: false },
		{ kind: 'coach', name: 'Endurance planner', role: 'Builds the 16-week plan', runs: 1, fresh: false },
		{ kind: 'coach', name: 'Habit coach', role: 'Notices missed sessions', runs: 1, fresh: false },
		{ kind: 'coach', name: 'Injury check', role: 'Safety rules for load and pain', runs: 1, fresh: false },
		{ kind: 'coach', name: 'Strength for runners', role: 'Writes the gym-day cards', runs: 1, fresh: true },
		{ kind: 'plugin', name: 'Run log', role: 'Pace, distance and weekly kilometres', runs: 2, fresh: true, version: 2 },
		{ kind: 'plugin', name: 'Body weight', role: 'Weekly weigh-in from Apple Health', runs: 1, fresh: false, version: 1 }
	],
	chat: [
		{ who: 'Running coach', text: 'Saturdays look busy for three weeks. Which day suits your long run?' },
		{ who: 'You', text: 'Sunday mornings are free but I have only 90 minutes.' },
		{ who: 'Running coach', text: '90 minutes covers up to 15 km at your pace. Answer the two questions above and I will move the long runs.' }
	]
};

/** The Settings mockup's values: signed-in account, Claude key, budgets, switches and counts. */
export const settingsSeed = {
	email: 'maya.k@icloud.com',
	signIn: 'Apple',
	server: 'Aroh hosted, aroh-ten.vercel.app',
	key: { vendor: 'Anthropic', last4: '4f2a', model: 'claude-sonnet-5-5' },
	budgetAll: '20',
	budgetGoal: '5',
	spent: '$3.40 of $20 this month',
	autonomy: 'Balanced',
	runLimit: 5,
	coaches: 14,
	calendars: true,
	reminders: true,
	health: false,
	notifications: 'Allowed',
	location: 'Always, 4 places',
	facts: 9,
	version: '0.6.0 (build 5)'
};
