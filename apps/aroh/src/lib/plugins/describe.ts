import type { PluginCadence, PluginField, PluginSummary, PluginTracker, PluginVersion } from './api';

const DAYS: Record<string, string> = { Mon: 'Monday', Tue: 'Tuesday', Wed: 'Wednesday', Thu: 'Thursday', Fri: 'Friday', Sat: 'Saturday', Sun: 'Sunday' };
const FIELD_KINDS: Record<string, string> = { number: 'number', text: 'text', bool: 'yes or no', choice: 'choice' };

type Item = Record<string, unknown>;

/** One section of a plugin dashboard in plain words: its title and one sentence per item. */
export type SectionWords = { title: string; items: string[] };

/** One tracker in plain words: its name, each field with its unit, and its cadence. */
export type TrackerWords = { name: string; fields: string[]; cadence: string };

/** One row of the Plugins group in the coach library: title, goal and version note, and the page link. */
export type PluginRow = { id: string; title: string; note: string; href: string };

/** Joins words as "a", "a and b" or "a, b and c"; `last` replaces "and" before the final word. */
function joined(words: string[], last = 'and'): string {
	if (words.length <= 1) return words.join('');
	return `${words.slice(0, -1).join(', ')} ${last} ${words[words.length - 1]}`;
}

/** The string at `key` of `item`, trimmed, or empty when it is not a non-empty string. */
function text(item: Item, key: string): string {
	const v = item[key];
	return typeof v === 'string' ? v.trim() : '';
}

/** The number at `key` of `item` as text, or empty when it is not a finite number. */
function num(item: Item, key: string): string {
	const v = item[key];
	return typeof v === 'number' && Number.isFinite(v) ? String(v) : '';
}

/** A check-in cadence in plain words, such as "Every Monday and Thursday at 20:00"; none gives "No set check-in time". */
export function cadenceText(c: PluginCadence | null | undefined): string {
	if (!c) return 'No set check-in time';
	if (c.freq === 'daily') return `Every day at ${c.at}`;
	const days = (c.weekdays ?? []).map((d) => DAYS[d] ?? d);
	return days.length ? `Every ${joined(days)} at ${c.at}` : `Once a week at ${c.at}`;
}

/** A tracker field as "Label (unit)", or "Label (kind)" when it has no unit. */
export function fieldText(f: PluginField): string {
	return `${f.label} (${f.unit?.trim() || FIELD_KINDS[f.kind] || f.kind})`;
}

/** A bundle's trackers in plain words: name, fields with units, cadence. */
export function trackerRows(trackers: PluginTracker[]): TrackerWords[] {
	return trackers.map((t) => ({ name: t.name, fields: t.fields.map(fieldText), cadence: cadenceText(t.cadence) }));
}

/** The tracker an item reads its data from, the first binding naming one, or empty. */
function source(item: Item): string {
	for (const v of Object.values(item)) {
		if (v && typeof v === 'object' && !Array.isArray(v) && typeof (v as Item).tracker === 'string') return (v as Item).tracker as string;
	}
	return '';
}

/** " in <unit>" when `item` has a unit, else empty. */
function unitOf(item: Item): string {
	const unit = text(item, 'unit');
	return unit ? ` in ${unit}` : '';
}

/** The kind name as words with a capital first letter, such as "Mystery kind". */
function kindWords(kind: string): string {
	const words = kind.replace(/[_-]+/g, ' ').trim() || 'item';
	return words[0].toUpperCase() + words.slice(1);
}

const KIND_WORDS: Record<string, (item: Item, label: string) => string> = {
	number_input: (i, l) => `Enter ${l}${unitOf(i)}`,
	rating: (i, l) => (num(i, 'min') && num(i, 'max') ? `Rate ${l} from ${num(i, 'min')} to ${num(i, 'max')}` : `Rate ${l}`),
	choice: (i, l) => {
		const options = Array.isArray(i.options) ? i.options.filter((o): o is string => typeof o === 'string') : [];
		if (!options.length) return `Pick: ${l}`;
		return `Pick ${i.multi === true ? 'any of' : 'one of'} ${joined(options, i.multi === true ? 'and' : 'or')}: ${l}`;
	},
	note: (_i, l) => `Write a note: ${l}`,
	check_off: (_i, l) => `Check off: ${l}`,
	counter: (i, l) => `Count ${l}${unitOf(i)}${num(i, 'target') ? ` toward ${num(i, 'target')}` : ''}`,
	timer: (i, l) => `Timer for ${l}${num(i, 'targetMinutes') ? `, ${num(i, 'targetMinutes')} min` : ''}`,
	capture: (i, l) => `Take a ${text(i, 'media') || 'photo'}: ${l}`,
	line_chart: (i, l) => `Line chart of ${l}${unitOf(i)}`,
	bar_chart: (i, l) => `Bar chart of ${l}${unitOf(i)}`,
	progress_ring: (i, l) => {
		const rings = Array.isArray(i.rings) ? i.rings.map((r) => (r && typeof r === 'object' ? text(r as Item, 'label') : '')).filter(Boolean) : [];
		return rings.length ? `Progress rings for ${l}: ${joined(rings)}` : `Progress rings for ${l}`;
	},
	target_vs_actual: (i, l) => `Target against actual for ${l}${unitOf(i)}`,
	streak_heatmap: (_i, l) => `Streak calendar of ${l}`,
	stat_tile: (i, l) => `Number tile of ${l}${unitOf(i)}`,
	log_list: (_i, l) => `List of entries: ${l}`,
	photo_timeline: (_i, l) => `Photo timeline: ${l}`,
	action_list: (_i, l) => `List of actions: ${l}`,
	week_strip: (_i, l) => `Week of actions: ${l}`,
	milestone_timeline: (i, l) => `Milestones: ${l}${Array.isArray(i.milestones) ? ` (${i.milestones.length})` : ''}`,
	module_checklist: (i, l) => `Module checklist: ${l}${Array.isArray(i.modules) ? ` (${i.modules.length})` : ''}`,
	countdown: (_i, l) => `Countdown: ${l}`,
	insight_card: (i) => `Insight from ${text(i, 'persona') || 'a coach'}: ${text(i, 'title') || text(i, 'body') || 'a note'}`,
	proposal_card: (i) => `Proposal from ${text(i, 'persona') || 'a coach'}: ${text(i, 'summary') || 'a change'}`,
	question_card: (i) => `Question from ${text(i, 'persona') || 'a coach'}: ${text(i, 'question') || 'a question'}`
};

/** One dashboard item in plain words, with the tracker it reads from when it names one. */
function itemText(item: Item): string {
	const kind = text(item, 'kind');
	const label = text(item, 'label') || text(item, 'title') || kindWords(kind);
	const words = KIND_WORDS[kind];
	if (!words) return label;
	const from = source(item);
	return from ? `${words(item, label)}, from the ${from} tracker` : words(item, label);
}

/** The dashboard sections of a validated plugin tree in plain words; a missing or malformed tree gives none. */
export function describeSections(tree: unknown): SectionWords[] {
	const sections = tree && typeof tree === 'object' ? (tree as Item).sections : null;
	if (!Array.isArray(sections)) return [];
	return sections
		.filter((s): s is Item => !!s && typeof s === 'object')
		.map((s) => ({
			title: text(s, 'title') || 'Section',
			items: (Array.isArray(s.items) ? s.items : []).filter((i): i is Item => !!i && typeof i === 'object').map(itemText)
		}));
}

/** The link of plugin row `id`'s page; `goalId` makes Back return to that goal. */
export function pluginPage(id: string, goalId?: string | null): string {
	return `/plugins/${encodeURIComponent(id)}${goalId ? `?goal=${encodeURIComponent(goalId)}` : ''}`;
}

/** The page link of plugin `name` among a goal's `versions`: the active version, else the newest; null when the goal has none. */
export function pluginPageFor(versions: PluginVersion[], name: string, goalId: string): string | null {
	const same = versions.filter((v) => v.name === name);
	const pick = same.find((v) => v.active) ?? same.reduce<PluginVersion | null>((best, v) => (!best || v.version > best.version ? v : best), null);
	return pick ? pluginPage(pick.id, goalId) : null;
}

/** The Plugins group rows of the coach library: title, "goal · name vN" (", active" when on) and the page link. */
export function pluginRows(list: PluginSummary[]): PluginRow[] {
	return list.map((p) => ({ id: p.id, title: p.title || p.name, note: `${p.goal_title ?? 'No goal'} · ${p.name} v${p.version}${p.active ? ', active' : ''}`, href: pluginPage(p.id) }));
}
