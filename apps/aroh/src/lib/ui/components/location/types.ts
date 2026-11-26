import type { CategorySlot } from '$lib/ui/contract';
import type { Tone } from '$lib/ui/components/StatusChip.svelte';

/** A goal or action a place is linked to, with the goal's category slot for its dot. */
export type PlaceLink = { goal: string; slot: CategorySlot; action?: string };

/**
 * One named place as the Places screen lists it. `watched` places are among the 20 iOS
 * monitors; `saved` ones are named but not watched right now; `ignored` ones are never used.
 */
export type PlaceRow = {
	id: string;
	name: string;
	address: string;
	links: PlaceLink[];
	state: 'watched' | 'saved' | 'ignored';
	visits: number;
	x: number;
	y: number;
};

/**
 * A place Aroh noticed from repeated visits, asking the owner what it is and what it serves.
 * `question` replaces "Is this your <guess>?" when no kind could be guessed.
 */
export type PlacePromptView = {
	id: string;
	guess: string;
	question?: string;
	name: string;
	address: string;
	visits: number;
	weeks: number;
	usual: string;
	targets: { value: string; label: string }[];
};

/** One map search result on the Places screen. */
export type PlaceResult = { id: string; name: string; address: string; distance: string; x: number; y: number };

/** One routine on the Routines dashboard. `id` is its series id; `streak` carries its unit; `tone` and `due` describe the next occurrence, `nextDue` and `nextId` (its session) null when none is planned (B96). */
export type RoutineRow = {
	id: string;
	title: string;
	place: string | null;
	cadence: string;
	streak: string;
	lastDone: string | null;
	nextDue: string | null;
	nextId: string | null;
	tone: Tone;
	due: string;
};

/** One raw visit from the phone's local log. Only visits to named places are listed. */
export type VisitRow = { id: string; place: string; arrived: string; left: string | null };

/** Location settings as the Settings > Location screen shows them. */
export type LocationSettingsView = {
	permission: 'always' | 'while_using' | 'off';
	paused: boolean;
	nudges: boolean;
	watched: number;
	visits: number;
	ignored: { id: string; name: string; address: string }[];
};
