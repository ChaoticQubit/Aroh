import { invoke } from '@tauri-apps/api/core';

/** A `goals` row. `target_date` is `YYYY-MM-DD`, meaning the end of that local day. */
export interface Goal {
	id: string;
	title: string;
	kind: 'course' | 'habit' | 'custom';
	source_url: string | null;
	target_date: string;
	status: 'draft' | 'active' | 'done' | 'archived';
	answers_json: string | null;
	created_at: string;
	updated_at: string;
	deleted: boolean;
}

/** An `actions` row. Times are UTC ISO 8601. */
export interface Action {
	id: string;
	goal_id: string;
	title: string;
	notes: string | null;
	start_at: string;
	end_at: string;
	est_minutes: number;
	status: 'planned' | 'done' | 'missed' | 'skipped';
	seq: number;
	done_at: string | null;
	updated_at: string;
	deleted: boolean;
}

/** A `plans` row. */
export interface Plan {
	id: string;
	goal_id: string;
	version: number;
	status: 'proposed' | 'approved' | 'rejected';
	summary: string | null;
	updated_at: string;
	deleted: boolean;
}

/** One course module with its estimated minutes. */
export interface CourseModule {
	title: string;
	minutes: number;
}

/** What the course analyzer found. */
export interface CourseAnalysis {
	title: string;
	modules: CourseModule[];
	total_minutes: number;
}

/** An intake question. Empty `options` means a free-text answer. */
export interface Question {
	id: string;
	text: string;
	options: string[];
}

/** The analyze step's result. */
export interface Analysis {
	analysis: CourseAnalysis;
	questions: Question[];
}

/** A plan with its proposed or approved actions. */
export interface PlanView {
	plan: Plan;
	actions: Action[];
}

/** Everything the goal detail screen shows. */
export interface GoalDetail {
	goal: Goal;
	plan: Plan | null;
	actions: Action[];
	proposed: Action[];
	analysis: CourseAnalysis | null;
	questions: Question[];
}

/** Prefix the backend puts on the error when no Claude key is saved. */
export const MISSING_KEY = 'missing-key: ';

/** True when `err` says the Claude key is missing, so the UI can link to Settings. */
export function isMissingKey(err: unknown): boolean {
	return String(err).startsWith(MISSING_KEY);
}

/** The error as one sentence for the screen, without the backend's code prefix. */
export function errorText(err: unknown): string {
	const text = String(err);
	return text.startsWith(MISSING_KEY) ? text.slice(MISSING_KEY.length) : text;
}

/** Creates a draft course goal from a link (or none, when the course text is pasted). */
export function createGoal(title: string, sourceUrl: string | null, targetDate: string): Promise<Goal> {
	return invoke<Goal>('goals_create', { title, sourceUrl, targetDate });
}

/** Reads and analyzes the course, then writes the intake questions. */
export function analyzeGoal(goalId: string, pastedText: string | null): Promise<Analysis> {
	return invoke<Analysis>('goals_analyze', { goalId, pastedText });
}

/** Saves the answers and generates a proposed plan. */
export function planGoal(goalId: string, answers: Record<string, string>): Promise<PlanView> {
	return invoke<PlanView>('goals_plan', { goalId, answers });
}

/** Approves a proposed plan: its actions join the calendar. */
export function approvePlan(planId: string): Promise<PlanView> {
	return invoke<PlanView>('goals_approve', { planId });
}

/** Loads one goal with its plan and actions. */
export function goalDetail(goalId: string): Promise<GoalDetail> {
	return invoke<GoalDetail>('goals_detail', { goalId });
}

/** Actions grouped by local day, in start order, with each day's total minutes. */
export function byDay(actions: Action[]): { day: Date; minutes: number; actions: Action[] }[] {
	const groups = new Map<string, { day: Date; minutes: number; actions: Action[] }>();
	const sorted = [...actions].sort((a, b) => a.start_at.localeCompare(b.start_at) || a.seq - b.seq);
	for (const action of sorted) {
		const start = new Date(action.start_at);
		const day = new Date(start.getFullYear(), start.getMonth(), start.getDate());
		const key = day.toDateString();
		const group = groups.get(key) ?? { day, minutes: 0, actions: [] };
		group.minutes += action.est_minutes;
		group.actions.push(action);
		groups.set(key, group);
	}
	return [...groups.values()];
}

/** Minutes as `45 min` or `1 h 30 min`. */
export function formatMinutes(total: number): string {
	console.assert(Number.isFinite(total) && total >= 0, 'minutes must be a non-negative number');
	const h = Math.floor(total / 60);
	const m = Math.round(total % 60);
	if (h === 0) return `${m} min`;
	return m === 0 ? `${h} h` : `${h} h ${m} min`;
}
