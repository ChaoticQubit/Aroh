/** Tracker units understood by inputs and charts. */
export type Unit = 'kg' | 'lb' | 'cm' | 'kcal' | 'g' | 'ml' | 'L' | 'min' | 'reps' | 'steps' | 'usd' | 'count';

/** One category colour slot, 1 to 6, mapped to --cat-N. */
export type CategorySlot = 1 | 2 | 3 | 4 | 5 | 6;

/** One chart series colour slot, 1 to 6, mapped to --series-N. */
export type SeriesSlot = 1 | 2 | 3 | 4 | 5 | 6;

/** One ring of a progress ring stack: a tracker's value toward its target, drawn in its series hue. */
export type Ring = { label: string; unit: Unit; value: number; target: number; series: SeriesSlot };

/** One point of a tracker series. */
export type Point = { at: string; value: number };

/** One planned or past action as the UI shows it. */
export type ActionView = {
	id: string;
	title: string;
	start: string;
	end: string;
	status: 'planned' | 'done' | 'missed' | 'skipped';
	locked: boolean;
	category: CategorySlot;
	goalTitle: string;
};

/** One change inside an agent proposal. */
export type ChangeView = {
	op: 'move' | 'resize' | 'create' | 'delete';
	title: string;
	before: string | null;
	after: string | null;
};

/** Renderer-neutral component spec: the shape plugin view trees use. */
export type ComponentSpec =
	| { kind: 'number_input'; label: string; unit: Unit | (string & {}); value: number | null; step: number; min?: number; max?: number }
	| { kind: 'rating'; label: string; min: number; max: number; value: number | null }
	| { kind: 'choice'; label: string; options: string[]; multi: boolean; value: string[] }
	| { kind: 'note'; label: string; placeholder: string; value: string }
	| { kind: 'check_off'; label: string; done: boolean }
	| { kind: 'counter'; label: string; unit: Unit; value: number; target: number }
	| { kind: 'timer'; label: string; targetMinutes: number }
	| { kind: 'capture'; label: string; media: 'photo' | 'audio' | 'video' }
	| { kind: 'line_chart'; label: string; unit: Unit; points: Point[]; target?: number; series?: SeriesSlot }
	| { kind: 'bar_chart'; label: string; unit: Unit; points: Point[]; target?: number; series?: SeriesSlot }
	| { kind: 'progress_ring'; label: string; rings: Ring[] }
	| { kind: 'target_vs_actual'; label: string; unit: Unit; target: number; actual: number }
	| { kind: 'streak_heatmap'; label: string; days: { date: string; level: 0 | 1 | 2 | 3 }[]; series?: SeriesSlot }
	| { kind: 'stat_tile'; label: string; unit: Unit; value: number; delta?: number }
	| { kind: 'log_list'; label: string; entries: { at: string; text: string; value?: number; unit?: Unit }[] }
	| { kind: 'photo_timeline'; label: string; photos: { at: string; src: string }[] }
	| { kind: 'action_list'; label: string; actions: ActionView[] }
	| { kind: 'week_strip'; label: string; actions: ActionView[]; startDay: string }
	| { kind: 'milestone_timeline'; label: string; milestones: { title: string; due: string; done: boolean }[] }
	| { kind: 'module_checklist'; label: string; modules: { title: string; minutes: number; done: boolean }[] }
	| { kind: 'countdown'; label: string; deadline: string }
	| { kind: 'insight_card'; persona: string; title: string; body: string }
	| { kind: 'proposal_card'; persona: string; summary: string; changes: ChangeView[] }
	| { kind: 'question_card'; persona: string; question: string; input: ComponentSpec };

/** The spec of one component kind, for example `SpecOf<'rating'>`. */
export type SpecOf<K extends ComponentSpec['kind']> = Extract<ComponentSpec, { kind: K }>;

/** The change callback every component accepts: the new value, shaped per kind. */
export type OnChange = (value: unknown) => void;

/** One entry of a persona chat thread: a message, an inline card, research progress, a working or stopped run, or a plan summary. */
export type ChatMessage =
	| { type: 'message'; from: 'owner' | 'persona'; name: string; text: string }
	| { type: 'card'; spec: ComponentSpec }
	| { type: 'progress'; lines: { text: string; done: boolean }[] }
	| { type: 'plan'; title: string; items: string[]; fixable?: boolean[] }
	| { type: 'working'; step: string | null; cost: string }
	| { type: 'stopped'; text: string; retry: boolean }
	| {
			type: 'questions';
			persona: string;
			prompt: string;
			items: { id: string; text: string; input: ComponentSpec }[];
			values: Record<string, string>;
			skipped: string[];
			missing: string[];
			status: string;
			submitted: boolean;
			sending: boolean;
			error: string | null;
	  };

/** A plugin view: a titled column of component specs, phone first. */
export type ViewSpec = { title: string; sections: { title: string; items: ComponentSpec[] }[] };
