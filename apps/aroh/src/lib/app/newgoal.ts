/** How freely agents change a goal's plan, as the engine names it. */
export type AutonomyLevel = 'ask' | 'balanced' | 'auto';

/** Autonomy levels with the label and the one line each shows under its name. */
export const AUTONOMY_LEVELS: { value: AutonomyLevel; label: string; meaning: string }[] = [
	{ value: 'ask', label: 'Ask', meaning: 'Every change waits for your approval.' },
	{ value: 'balanced', label: 'Balanced', meaning: 'Small moves apply with undo. Big changes ask first.' },
	{ value: 'auto', label: 'Auto', meaning: 'All changes apply with undo, plus a daily summary.' }
];

/** The agent controls of one goal, or the Settings defaults. `runLimit` null uses the Settings run limit. */
export type AgentSettings = { autonomy: AutonomyLevel; reminders: boolean; useCalendars: boolean; runLimit: number | null };

/**
 * What the New Goal form holds. `target` is `YYYY-MM-DD` or empty; `categoryPicked` is true once the
 * owner chose a category, so a later guess from the title no longer replaces it.
 */
export type NewGoalDraft = AgentSettings & { title: string; description: string; target: string; categoryId: string | null; categoryPicked: boolean };

/** Body of `POST /v1/goals` built from the form. */
export type StartGoalBody = {
	goal_id?: string;
	title: string;
	context?: string;
	deadline?: string;
	category_id?: string;
	autonomy: AutonomyLevel;
	reminders: boolean;
	use_calendars: boolean;
	run_limit_minutes?: number;
	timezone: string;
};

const MAX_TITLE = 200;

const BUILT_IN: AgentSettings = { autonomy: 'balanced', reminders: true, useCalendars: true, runLimit: null };

/**
 * The agent controls new goals start with, from the `agent_defaults` setting (JSON with
 * `autonomy`, `reminders`, `use_calendars`). A missing or bad value gives the built-in
 * defaults. `runLimit` is always null: the Settings run limit applies.
 */
export function agentDefaults(raw: string | undefined): AgentSettings {
	try {
		const v = JSON.parse(raw ?? '') as { autonomy?: unknown; reminders?: unknown; use_calendars?: unknown };
		const level = AUTONOMY_LEVELS.find((a) => a.value === v.autonomy)?.value;
		if (!level || typeof v.reminders !== 'boolean' || typeof v.use_calendars !== 'boolean') return { ...BUILT_IN };
		return { autonomy: level, reminders: v.reminders, useCalendars: v.use_calendars, runLimit: null };
	} catch {
		return { ...BUILT_IN };
	}
}

/** The `agent_defaults` setting value for `s`, leaving out the run limit, which Settings keeps on the engine. */
export function defaultsValue(s: AgentSettings): string {
	return JSON.stringify({ autonomy: s.autonomy, reminders: s.reminders, use_calendars: s.useCalendars });
}

/** An empty form with the default agent controls, or `defaults` when Settings has them. */
export function blankDraft(defaults: AgentSettings = BUILT_IN): NewGoalDraft {
	return { title: '', description: '', target: '', categoryId: null, categoryPicked: false, ...defaults };
}

/** Why the form cannot be created yet, in the owner's words, or null when it can. */
export function problem(d: NewGoalDraft): string | null {
	const title = d.title.trim();
	if (!title) return 'Add a title to create the goal.';
	if (title.length > MAX_TITLE) return `Keep the title under ${MAX_TITLE} characters.`;
	if (d.target && !/^\d{4}-\d{2}-\d{2}$/.test(d.target)) return 'Pick a target date.';
	if (d.runLimit !== null && !(Number.isInteger(d.runLimit) && d.runLimit >= 1 && d.runLimit <= 120)) return 'The run time limit is 1 to 120 minutes.';
	return null;
}

/** True when Create may be pressed. */
export function canCreate(d: NewGoalDraft): boolean {
	return problem(d) === null;
}

/** The request body for the form in the owner's IANA `zone`; empty optional fields are left out. */
export function startBody(d: NewGoalDraft, zone: string): StartGoalBody {
	console.assert(canCreate(d), 'startBody needs a valid form');
	const body: StartGoalBody = { title: d.title.trim(), autonomy: d.autonomy, reminders: d.reminders, use_calendars: d.useCalendars, timezone: zone };
	const context = d.description.trim();
	if (context) body.context = context;
	if (d.target) body.deadline = d.target;
	if (d.categoryId) body.category_id = d.categoryId;
	if (d.runLimit !== null) body.run_limit_minutes = d.runLimit;
	return body;
}

/** The owner sets the target date to `ymd` (`YYYY-MM-DD`), from a quick pick or the date input. */
export function withTarget(d: NewGoalDraft, ymd: string): NewGoalDraft {
	console.assert(/^\d{4}-\d{2}-\d{2}$/.test(ymd), 'withTarget needs YYYY-MM-DD');
	return { ...d, target: ymd };
}

/** The owner chooses No end date: the target is cleared, so no deadline is sent. */
export function noEndDate(d: NewGoalDraft): NewGoalDraft {
	return { ...d, target: '' };
}

/** Applies a category guess (an id or null) unless the owner already picked a category. */
export function withGuess(d: NewGoalDraft, guessed: string | null): NewGoalDraft {
	return d.categoryPicked ? d : { ...d, categoryId: guessed };
}

/** The owner picks category `id`; later guesses keep it. */
export function pickCategory(d: NewGoalDraft, id: string): NewGoalDraft {
	return { ...d, categoryId: id, categoryPicked: true };
}

function ymd(y: number, m: number, day: number): string {
	const last = new Date(Date.UTC(y, m + 1, 0)).getUTCDate();
	const at = new Date(Date.UTC(y, m, Math.min(day, last)));
	return at.toISOString().slice(0, 10);
}

/** Quick target dates from `today` (`YYYY-MM-DD` in the owner zone): 1 and 3 months on, and the end of the year. */
export function quickPicks(today: string): { label: string; ymd: string }[] {
	const [y, m, d] = today.split('-').map(Number);
	const endOfYear = `${y}-12-31`;
	return [
		{ label: 'In 1 month', ymd: ymd(y, m, d) },
		{ label: 'In 3 months', ymd: ymd(y, m + 2, d) },
		endOfYear > today ? { label: 'End of year', ymd: endOfYear } : { label: 'End of next year', ymd: `${y + 1}-12-31` }
	];
}
