import type { Tone } from '../components/StatusChip.svelte';
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
	lead: { unit: Unit; now: number; target?: number } | null;
};

/** One Inbox card with the id of the row a decision applies to. */
export type InboxItem = { id: string; spec: ComponentSpec };

/** One agent run in progress, as the Inbox lists it. */
export type RunView = { persona: string; title: string; step: string; progress: number; tone: Tone; working: boolean; href?: string };

/** One model provider of the Settings dropdown. */
export type ProviderView = { id: string; name: string; protocol: string; models: string[]; keySet: boolean; last4: string | null; selfHosted: boolean };

/** Everything the Settings screen shows. Numbers are US dollars. */
export type SettingsView = {
	providers: ProviderView[];
	providerId: string;
	model: string;
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

/** What an all-day event does to its days (Q47): `reminder` only shows, `busy` clears them, `away` adapts the plan. */
export type DayKind = 'reminder' | 'busy' | 'away';

/**
 * A device all-day event from `start` to `end`, both "YYYY-MM-DD" and inclusive. `kind` is null
 * until the owner confirms once; `suggested` is the triage persona's proposal, if any.
 */
export type AllDayView = { id: string; title: string; start: string; end: string; kind: DayKind | null; suggested?: DayKind | null };

/** The owner's pick of a kind for the all-day event `id`. */
export type DayPick = { id: string; kind: DayKind };

/** An edit of one action from the Calendar or Today: its new start and end instants. */
export type ActionEdit = { id: string; start: string; end: string };

/** A goal row of the Goals list. */
export type GoalRow = { id: string; title: string; category: string; slot: CategorySlot; deadline: string | null; autonomy: AutonomyLabel };

/**
 * One coach (persona) as screens show it; `locked` are the safety rules a copy keeps. On a goal page `runs` counts the goal's runs it
 * worked in and `fresh` is true when an agent created it during one of them (Q72).
 */
export type CoachView = { name: string; display_name: string; role: string; instructions: string; guardrails: string[]; locked: string[]; source: 'builtin' | 'library'; runs?: number; fresh?: boolean };

/** A plugin a goal's runs used: name, the newest version used, runs that used it, and `fresh` when a run of this goal made it. */
export type PluginUsed = { name: string; version: number; runs: number; fresh: boolean };

/** One team's coaches under a header with the dot of its category slot, or none for Shared. */
export type CoachGroup = { title: string; slot: CategorySlot | null; personas: CoachView[] };

/** The editable text of a coach copy: name, style, instructions and rules. */
export type CoachFields = { display_name: string; role: string; instructions: string; guardrails: string[] };

/** One kept profile fact: its text, where it came from, its ISO date, and whether it can be blocked (it has a topic). */
export type FactView = { id: string; text: string; source: string; date: string; blockable: boolean };

/** "What Aroh knows about you": kept facts newest first, and the topics the owner never wants learned. */
export type ProfileView = { facts: FactView[]; blocked: { id: string; label: string }[] };
