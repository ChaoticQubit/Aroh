/** A UI slot name, as listed in docs/plugins.md. */
export type SlotName = 'today.top' | 'today.sidebar' | 'goal.detail' | 'log.trackers';

/** A component name from the fixed component list. */
export type ComponentName =
	| 'stat-tile'
	| 'progress-bar'
	| 'checklist'
	| 'action-list'
	| 'quick-chips'
	| 'text-note';

/** A prop bound to a host-computed query by name. */
export interface QueryRef {
	query: string;
}

/** A resolved prop: a literal text or number, or a query reference. */
export type PropValue = string | number | QueryRef;

/** Name, version and title of a loaded plugin. */
export interface PluginInfo {
	name: string;
	version: string;
	title: string;
}

/** One component instance placed in a slot. `component` stays a string so unknown names can be skipped. */
export interface RenderInstance {
	key: string;
	plugin: string;
	component: ComponentName | string;
	props: Record<string, PropValue>;
}

/** A tracker type declared by a plugin. */
export interface RenderTracker {
	plugin: string;
	id: string;
	kind: 'food' | 'water' | 'workout' | 'other';
	unit: string;
	chips: number[];
	daily_target: number | null;
}

/** An agent declared by a plugin. */
export interface RenderAgent {
	plugin: string;
	name: string;
	purpose: string;
	tools: string[];
	variables: string[];
	untrusted: string[];
	effort: 'low' | 'medium' | 'high';
	output: string | null;
	system: string | null;
	prompt: string;
}

/** The JSON render model produced by aroh_plugins::resolve. Every slot key is present. */
export interface RenderModel {
	plugins: PluginInfo[];
	slots: Record<SlotName, RenderInstance[]>;
	trackers: RenderTracker[];
	agents: RenderAgent[];
}

/** Query results the host passes in, keyed by query name. */
export type QueryData = Record<string, number | unknown[]>;

/** Called when a quick chip is tapped: log `amount` into `tracker`. */
export type LogHandler = (tracker: string, amount: number) => void;

/** Called when a checklist box changes: mark action `id` done or not. */
export type ToggleHandler = (id: string, done: boolean) => void;
