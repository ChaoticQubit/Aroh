import type { PropValue, QueryData } from './types';

/** A calendar action row read defensively from a list query item. */
export interface ActionRow {
	id: string;
	title: string;
	startAt: string | null;
	status: string;
	done: boolean;
}

/** True when `v` is a `{ query: name }` reference. */
export function isQuery(v: unknown): v is { query: string } {
	return typeof v === 'object' && v !== null && typeof (v as { query?: unknown }).query === 'string';
}

/** Resolves a number prop: a finite literal, or a finite number query result. Anything else is null. */
export function numberProp(v: PropValue | undefined, data: QueryData): number | null {
	const raw = isQuery(v) ? data[v.query] : v;
	return typeof raw === 'number' && Number.isFinite(raw) ? raw : null;
}

/** Resolves a list prop from a list query result. Anything else is null. */
export function listProp(v: PropValue | undefined, data: QueryData): unknown[] | null {
	const raw = isQuery(v) ? data[v.query] : undefined;
	return Array.isArray(raw) ? raw : null;
}

/** Returns a text prop, or the empty string when the prop is not text. */
export function textProp(v: PropValue | undefined): string {
	return typeof v === 'string' ? v : '';
}

/** Fills `value / max` clamped to 0..1. Missing values or a max of 0 or less give 0. */
export function fraction(value: number | null, max: number | null): number {
	if (value === null || max === null || max <= 0) return 0;
	const f = value / max;
	return Math.min(1, Math.max(0, f));
}

/** Formats a number for display, with a dash for a missing value. */
export function showNumber(n: number | null): string {
	if (n === null) return '-';
	return Number.isInteger(n) ? n.toLocaleString() : n.toLocaleString(undefined, { maximumFractionDigits: 2 });
}

/** Reads a list item as an action row, or null when it has no usable title. */
export function actionRow(item: unknown, index: number): ActionRow | null {
	if (typeof item !== 'object' || item === null) return null;
	const o = item as Record<string, unknown>;
	if (typeof o.title !== 'string' || o.title === '') return null;
	const status = typeof o.status === 'string' ? o.status : 'planned';
	return {
		id: typeof o.id === 'string' ? o.id : `row-${index}`,
		title: o.title,
		startAt: typeof o.start_at === 'string' ? o.start_at : null,
		status,
		done: status === 'done'
	};
}

/** Formats an ISO time as a local hour and minute, or the empty string when it is not a valid time. */
export function showTime(iso: string | null): string {
	if (iso === null) return '';
	const d = new Date(iso);
	return Number.isNaN(d.getTime()) ? '' : d.toLocaleTimeString(undefined, { hour: 'numeric', minute: '2-digit' });
}
