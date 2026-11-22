import type { CategorySlot, ComponentSpec, Unit } from '../contract';

/** Autonomy level as the screens label it (Q19). */
export type AutonomyLabel = 'Ask' | 'Balanced' | 'Auto';

/** A seeded or owner-defined category with its colour slot. */
export type CategoryView = { name: string; slot: CategorySlot };

/** The header facts of one goal: category, deadline, autonomy, personas and its lead tracker. */
export type GoalHeader = {
	id: string;
	title: string;
	category: string;
	slot: CategorySlot;
	deadline: string | null;
	autonomy: AutonomyLabel;
	personas: string[];
	lead: { unit: Unit; now: number; target?: number } | null;
};

/** One Inbox card with the id of the row a decision applies to. */
export type InboxItem = { id: string; spec: ComponentSpec };

/** One agent run in progress, as the Inbox lists it. */
export type RunView = { persona: string; title: string; step: string; progress: number };

/** One model provider of the Settings dropdown. */
export type ProviderView = { id: string; name: string; protocol: string; models: string[]; keySet: boolean; last4: string | null; selfHosted: boolean };

/** One configured web search provider, in fallback order. */
export type SearchView = { name: string; detail: string; keySet: boolean };

/** Everything the Settings screen shows. Numbers are US dollars. */
export type SettingsView = {
	providers: ProviderView[];
	providerId: string;
	model: string;
	search: SearchView[];
	serverUrl: string;
	budgetAll: number | null;
	budgetGoal: number | null;
	calendars: boolean;
	health: boolean;
	account: AccountView;
};

/** The Account card: Supabase project, and the signed-in email or null. */
export type AccountView = { supabaseUrl: string; anonKey: string; email: string | null };

/** A kept device calendar event on the Calendar: neutral busy time with its triage group. */
export type BusyView = { id: string; title: string; start: string; end: string; group: string };

/** A goal row of the Goals list. */
export type GoalRow = { id: string; title: string; category: string; slot: CategorySlot; deadline: string | null; autonomy: AutonomyLabel };

/** One coach (persona) as the library screens show it; `locked` are the safety rules a copy keeps. */
export type CoachView = { name: string; display_name: string; role: string; instructions: string; guardrails: string[]; locked: string[]; source: 'builtin' | 'library' };

/** One team's coaches under a header with the dot of its category slot, or none for Shared. */
export type CoachGroup = { title: string; slot: CategorySlot | null; personas: CoachView[] };

/** The editable text of a coach copy: name, style, instructions and rules. */
export type CoachFields = { display_name: string; role: string; instructions: string; guardrails: string[] };
