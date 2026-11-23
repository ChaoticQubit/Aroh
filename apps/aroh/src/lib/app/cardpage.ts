import type { Action, ActionLog, Snapshot } from './client';
import type { Card, CardLog, CardMeta } from '$lib/ui/action/card';
import { actionViews } from './views';
import { formatTime, formatYmd } from '$lib/ui/format';

const WEEK_MS = 7 * 24 * 3600 * 1000;

/** The empty session log a card starts from. */
export const emptyLog = (): CardLog => ({ sets: [], checked: [], effort: null, note: null });

/** The editable part of a stored log. */
export function logOf(stored: ActionLog | null): CardLog {
	return stored ? { sets: stored.sets, checked: stored.checked, effort: stored.effort, note: stored.note } : emptyLog();
}

/**
 * Header facts of the card of `action` in `zone`: goal title and category hue from the snapshot,
 * local date and start, length, and the week of the goal counted from its creation.
 */
export function cardMeta(s: Snapshot, action: Action, zone: string): CardMeta {
	const view = actionViews({ ...s, actions: [action] })[0];
	const goal = s.goals.find((g) => g.id === action.goal_id);
	const minutes = Math.round((Date.parse(action.end_at) - Date.parse(action.start_at)) / 60000);
	const week = goal ? Math.max(1, Math.floor((Date.parse(action.start_at) - Date.parse(goal.created_at)) / WEEK_MS) + 1) : 1;
	console.assert(minutes > 0, 'an action lasts more than 0 minutes');
	return { title: action.title, goal: goal?.title ?? '', hue: view.category, date: formatYmd(action.start_at, zone), start: formatTime(action.start_at, zone), minutes, week: `Week ${week}`, coach: 'Your coach' };
}

/**
 * The card to show and edit for `action`: its own card, else a blank one with an empty step list
 * the owner fills by typing (the weekly fill writes it otherwise).
 */
export function pageCard(action: Action): Card {
	return action.card ?? { v: 1, depth: 'full', why: '', blocks: [{ kind: 'steps', title: 'Steps', items: [{ text: '', minutes: null }] }], outline: null };
}

/** The date the weekly fill writes an outline card in full: one week before the session, never in the past. */
export function fillDate(action: Action, zone: string, today: string): string {
	const day = formatYmd(new Date(Date.parse(action.start_at) - WEEK_MS).toISOString(), zone);
	return day < today ? today : day;
}

/** Plain words of a scope for the chat diff card and the Undo bar. */
export function scopeText(scope: 'this' | 'future'): string {
	return scope === 'this' ? 'This session' : 'This and future sessions';
}
