import type { ActionView } from '$lib/ui/contract';
import type { DiaryDayView, DiaryEntryView } from '$lib/app/diary';

export const diaryNow = '2026-11-26T21:30:00Z';

const todayEntries: DiaryEntryView[] = [
	{ id: 'e1', at: '2026-11-26T07:45:00Z', text: 'Slept badly, maybe six hours. Core session anyway, felt slow but finished it.', source: 'text' },
	{ id: 'e2', at: '2026-11-26T13:10:00Z', text: 'Lunch was the chicken rice bowl again. Two big glasses of water since the morning, should drink more.', source: 'voice' },
	{ id: 'e3', at: '2026-11-26T18:40:00Z', text: 'Good call with Maya about the April trip. Work was calm, finished the budget sheet early.', source: 'text' },
	{ id: 'e4', at: '2026-11-26T21:20:00Z', text: 'Piano for twenty minutes, the left hand part finally clicked. Tired but happy. Tomorrow: stretch before the strength session and go to bed by eleven.', source: 'voice' }
];

/** Today, fully structured after the fourth entry. */
export const today: DiaryDayView = {
	day: '2026-11-26',
	entries: todayEntries,
	structure: {
		summary: 'A tired start after a short night, but a steady day: the core session done, a calm workday and a good evening at the piano.',
		mood: 4,
		energy: 2,
		highlights: ['Left hand part of the piece finally clicked', 'Planned the April trip with Maya', 'Budget sheet finished early'],
		food: 'Chicken rice bowl for lunch',
		water: '2 glasses, about 0.5 l',
		workout: 'Core session, 30 min',
		notesForTomorrow: 'Stretch before the strength session. In bed by 23:00.',
		structuredAt: '2026-11-26T21:21:00Z'
	},
	done: [
		{ id: 'a-2026-11-26-0700-2', title: 'Core session', start: '2026-11-26T07:00:00Z', category: 2 },
		{ id: 'a-2026-11-26-2000-4', title: 'Piano practice', start: '2026-11-26T20:00:00Z', category: 4 }
	]
};

/** Today with two entries and no Claude key on this phone: saved encrypted, not structured. */
export const todayUnstructured: DiaryDayView = { day: '2026-11-26', entries: todayEntries.slice(0, 2), structure: null, done: today.done.slice(0, 1) };

/** Tuesday 24 November, structured, shown on the calendar day view. */
export const tuesday: DiaryDayView = {
	day: '2026-11-24',
	entries: [
		{ id: 't1', at: '2026-11-24T07:40:00Z', text: 'Core session done before breakfast, strong today.', source: 'text' },
		{ id: 't2', at: '2026-11-24T20:45:00Z', text: 'Long day at work, skipped the walk. Piano was relaxing. Ate too late again.', source: 'voice' }
	],
	structure: {
		summary: 'Strong morning workout, a long workday and a relaxed piano session. Dinner came late again.',
		mood: 3,
		energy: 4,
		highlights: ['Core session felt strong'],
		food: 'Late dinner',
		water: null,
		workout: 'Core session, 30 min',
		notesForTomorrow: 'Eat dinner before 20:00.',
		structuredAt: '2026-11-24T20:46:00Z'
	},
	done: [
		{ id: 'a-2026-11-24-0700-2', title: 'Core session', start: '2026-11-24T07:00:00Z', category: 2 },
		{ id: 'a-2026-11-24-2000-4', title: 'Piano practice', start: '2026-11-24T20:00:00Z', category: 4 }
	]
};

/** Wednesday 25 November, structured, a past day under today on the Log tab. */
export const wednesday: DiaryDayView = {
	day: '2026-11-25',
	entries: [{ id: 'w1', at: '2026-11-25T21:00:00Z', text: 'Missed the gym, meeting ran late. Felt flat all evening.', source: 'text' }],
	structure: {
		summary: 'A meeting ran late and the strength session was missed. Low evening.',
		mood: 2,
		energy: 2,
		highlights: [],
		food: null,
		water: null,
		workout: null,
		notesForTomorrow: 'Book the gym slot before the meeting.',
		structuredAt: '2026-11-25T21:01:00Z'
	},
	done: [{ id: 'a-2026-11-25-2000-4', title: 'Piano practice', start: '2026-11-25T20:00:00Z', category: 4 }]
};

/** The sessions on Tuesday 24 November for the calendar day frame. */
export const tuesdayActions: ActionView[] = [
	{ id: 'a-2026-11-24-0700-2', title: 'Core session', start: '2026-11-24T07:00:00Z', end: '2026-11-24T07:30:00Z', status: 'done', locked: false, category: 2, goalTitle: 'Six pack by Feb 1' },
	{ id: 'a-2026-11-24-2000-4', title: 'Piano practice', start: '2026-11-24T20:00:00Z', end: '2026-11-24T20:30:00Z', status: 'done', locked: false, category: 4, goalTitle: 'Learn piano' }
];
