import type { ViewSpec } from '$lib/ui/contract';
import type { LocationSettingsView, PlacePromptView, PlaceResult, PlaceRow, RoutineRow, VisitRow } from '$lib/ui/components/location/types';
import type { GoalHeader, InboxItem } from '$lib/ui/screens/types';
import { shiftDay } from '$lib/ui/format';

/** The gym Aroh noticed from repeated evening visits, asked about in the Inbox. */
export const gymPrompt: PlacePromptView = {
	id: 'pp-gym',
	guess: 'gym',
	name: 'Iron Works Gym',
	address: '1200 Market St',
	visits: 6,
	weeks: 3,
	usual: 'usually Monday, Wednesday and Friday around 18:00',
	targets: [
		{ value: 'a-strength', label: 'Strength session, Six-pack by summer' },
		{ value: 'g-sixpack', label: 'Six-pack by summer, any action' },
		{ value: 'g-routines', label: 'Routines' }
	]
};

/** The learned grocery routine, proposed through the existing proposal card. */
export const groceryPattern: InboxItem = {
	id: 'pat-groceries',
	spec: {
		kind: 'proposal_card',
		persona: 'Planner',
		summary: "You buy groceries at Trader Joe's about every 7 days, usually Saturday ~10:00. Make it a weekly action?",
		changes: [{ op: 'create', title: "Groceries at Trader Joe's, Routines", before: null, after: 'Every Saturday 10:00, 45 min' }]
	}
};

/** Named places: four watched, one named but not watched, one ignored. */
export const places: PlaceRow[] = [
	{ id: 'pl-gym', name: 'Iron Works Gym', address: '1200 Market St', links: [{ goal: 'Six-pack by summer', slot: 2, action: 'Strength session' }], state: 'watched', visits: 14, x: 190, y: 100 },
	{ id: 'pl-tj', name: "Trader Joe's", address: '555 9th St', links: [{ goal: 'Routines', slot: 6, action: 'Groceries' }], state: 'watched', visits: 9, x: 300, y: 60 },
	{ id: 'pl-studio', name: 'Piano studio', address: '48 Valencia St', links: [{ goal: 'Learn piano', slot: 4 }], state: 'watched', visits: 5, x: 70, y: 140 },
	{ id: 'pl-library', name: 'Mission library', address: '300 Bartlett St', links: [{ goal: 'Read 20 books', slot: 1 }, { goal: 'Routines', slot: 6, action: 'Return books' }], state: 'watched', visits: 4, x: 120, y: 60 },
	{ id: 'pl-pool', name: 'City pool', address: '19th and Linda St', links: [], state: 'saved', visits: 2, x: 250, y: 160 },
	{ id: 'pl-office', name: 'Office', address: '1 Front St', links: [], state: 'ignored', visits: 41, x: 330, y: 150 }
];

/** Map search results for "trader". */
export const placeResults: PlaceResult[] = [
	{ id: 'r-1', name: "Trader Joe's", address: '555 9th St', distance: '1.2 km', x: 300, y: 60 },
	{ id: 'r-2', name: "Trader Joe's", address: '3 Masonic Ave', distance: '4.8 km', x: 60, y: 40 },
	{ id: 'r-3', name: 'Trader Vic Market', address: '210 Ellis St', distance: '2.1 km', x: 190, y: 150 }
];

/** The built-in Routines goal in the Habit category. */
export const routinesHeader: GoalHeader = {
	id: 'g-routines',
	title: 'Routines',
	category: 'Habit',
	slot: 6,
	deadline: null,
	autonomy: 'Ask',
	lead: null
};

/** Routines as of Thursday 26 Nov: groceries due Saturday, books missed, a morning walk with no place. */
export const routines: RoutineRow[] = [
	{ id: 'rt-groceries', title: 'Groceries', place: "Trader Joe's", cadence: 'Weekly, Sat', streak: '6 weeks', lastDone: '2026-11-21T10:05:00Z', nextDue: '2026-11-28T10:00:00Z', tone: 'planned', due: 'Planned' },
	{ id: 'rt-books', title: 'Return books', place: 'Mission library', cadence: 'Every 3 weeks', streak: '2 times', lastDone: '2026-11-02T17:30:00Z', nextDue: '2026-11-23T17:30:00Z', tone: 'missed', due: 'Missed' },
	{ id: 'rt-walk', title: 'Morning walk', place: null, cadence: 'Daily', streak: '9 days', lastDone: '2026-11-25T07:10:00Z', nextDue: '2026-11-26T07:00:00Z', tone: 'warn', due: 'Due today' }
];

/** Routines dashboard view: two tiles and the last four weeks of done routines. */
export const routinesView: ViewSpec = {
	title: 'Routines',
	sections: [
		{
			title: 'Last 4 weeks',
			items: [
				{ kind: 'stat_tile', label: 'Done this week', unit: 'count', value: 4, delta: 1 },
				{ kind: 'stat_tile', label: 'Longest streak', unit: 'count', value: 9 },
				{
					kind: 'streak_heatmap',
					label: 'All routines',
					series: 2,
					days: Array.from({ length: 28 }, (_, i) => ({ date: shiftDay('2026-10-30', i), level: ([1, 2, 0, 1, 3, 1, 2][i % 7] ?? 0) as 0 | 1 | 2 | 3 }))
				}
			]
		}
	]
};

/** Location settings: Always access, tracking on, one ignored place. */
export const locationSettings: LocationSettingsView = {
	permission: 'always',
	paused: false,
	nudges: true,
	watched: 4,
	visits: 214,
	ignored: [{ id: 'pl-office', name: 'Office', address: '1 Front St' }]
};

/** Recent visits to named places, newest last; the gym visit is still open. */
export const visits: VisitRow[] = [
	{ id: 'v-1', place: "Trader Joe's", arrived: '2026-11-21T10:05:00Z', left: '2026-11-21T10:48:00Z' },
	{ id: 'v-2', place: 'Piano studio', arrived: '2026-11-23T19:00:00Z', left: '2026-11-23T20:02:00Z' },
	{ id: 'v-3', place: 'Iron Works Gym', arrived: '2026-11-23T17:58:00Z', left: '2026-11-23T19:04:00Z' },
	{ id: 'v-4', place: 'Iron Works Gym', arrived: '2026-11-25T18:02:00Z', left: '2026-11-25T19:10:00Z' },
	{ id: 'v-5', place: 'Mission library', arrived: '2026-11-25T12:30:00Z', left: '2026-11-25T12:52:00Z' },
	{ id: 'v-6', place: 'Iron Works Gym', arrived: '2026-11-26T18:01:00Z', left: null }
];
