import type { ComponentSpec } from '../contract';
import { sixpackActions, sixpackEntries, sixpackInbox, sixpackStreak, weekRings, weightCheckIn } from './sixpack';

/** One gallery tile: the state it shows and the spec that produces it. */
export type GalleryItem = { state: 'empty' | 'filled' | 'over target' | 'missed'; spec: ComponentSpec };

const today = sixpackActions.filter((a) => a.start.startsWith('2026-11-26'));
const wednesday = sixpackActions.filter((a) => a.start.startsWith('2026-11-25'));

/** Every component kind in each state that applies, grouped by family. */
export const gallery: Record<'inputs' | 'display' | 'planning' | 'agent', GalleryItem[]> = {
	inputs: [
		{ state: 'empty', spec: { kind: 'number_input', label: 'Weight', unit: 'kg', value: null, step: 0.1, min: 30, max: 250 } },
		{ state: 'filled', spec: { kind: 'number_input', label: 'Weight', unit: 'kg', value: 82.4, step: 0.1, min: 30, max: 250 } },
		{ state: 'empty', spec: { kind: 'rating', label: 'Energy today', min: 1, max: 5, value: null } },
		{ state: 'filled', spec: { kind: 'rating', label: 'Energy today', min: 1, max: 5, value: 4 } },
		{ state: 'empty', spec: { kind: 'choice', label: 'Training days', options: ['Mon', 'Tue', 'Wed', 'Thu', 'Fri', 'Sat', 'Sun'], multi: true, value: [] } },
		{ state: 'filled', spec: { kind: 'choice', label: 'Training days', options: ['Mon', 'Tue', 'Wed', 'Thu', 'Fri', 'Sat', 'Sun'], multi: true, value: ['Mon', 'Wed', 'Fri'] } },
		{ state: 'empty', spec: { kind: 'note', label: 'How did the session feel?', placeholder: 'Optional', value: '' } },
		{ state: 'filled', spec: { kind: 'note', label: 'How did the session feel?', placeholder: 'Optional', value: 'Heavy on the last set of squats, form held.' } },
		{ state: 'empty', spec: { kind: 'check_off', label: 'Drink 2 L of water', done: false } },
		{ state: 'filled', spec: { kind: 'check_off', label: 'Drink 2 L of water', done: true } },
		{ state: 'filled', spec: { kind: 'counter', label: 'Glasses of water', unit: 'count', value: 5, target: 8 } },
		{ state: 'over target', spec: { kind: 'counter', label: 'Glasses of water', unit: 'count', value: 9, target: 8 } },
		{ state: 'empty', spec: { kind: 'timer', label: 'Plank hold', targetMinutes: 3 } },
		{ state: 'empty', spec: { kind: 'capture', label: 'Progress photo', media: 'photo' } },
		{ state: 'empty', spec: { kind: 'capture', label: 'Voice note', media: 'audio' } }
	],
	display: [
		{ state: 'filled', spec: { kind: 'line_chart', label: 'Weight', unit: 'kg', points: sixpackEntries.weight, target: 78, series: 3 } },
		{ state: 'empty', spec: { kind: 'line_chart', label: 'Weight', unit: 'kg', points: [], target: 78 } },
		{ state: 'filled', spec: { kind: 'bar_chart', label: 'Protein per day', unit: 'g', points: sixpackEntries.protein, target: 150, series: 2 } },
		{ state: 'empty', spec: { kind: 'bar_chart', label: 'Protein per day', unit: 'g', points: [] } },
		{ state: 'filled', spec: { kind: 'progress_ring', label: 'This week', rings: weekRings } },
		{ state: 'over target', spec: { kind: 'progress_ring', label: 'Workouts this week', rings: [{ label: 'Workouts', unit: 'count', value: 5, target: 4, series: 1 }] } },
		{ state: 'filled', spec: { kind: 'target_vs_actual', label: 'Protein today', unit: 'g', target: 150, actual: 141 } },
		{ state: 'over target', spec: { kind: 'target_vs_actual', label: 'Calories today', unit: 'kcal', target: 2200, actual: 2450 } },
		{ state: 'filled', spec: { kind: 'streak_heatmap', label: 'Last 4 weeks', days: sixpackStreak, series: 2 } },
		{ state: 'empty', spec: { kind: 'streak_heatmap', label: 'Last 4 weeks', days: [] } },
		{ state: 'filled', spec: { kind: 'stat_tile', label: 'Weight', unit: 'kg', value: 82.4, delta: -0.6 } },
		{ state: 'empty', spec: { kind: 'stat_tile', label: 'Workouts', unit: 'count', value: 0 } },
		{ state: 'filled', spec: { kind: 'log_list', label: 'Meals', entries: sixpackEntries.meals } },
		{ state: 'empty', spec: { kind: 'log_list', label: 'Meals', entries: [] } },
		{
			state: 'filled',
			spec: {
				kind: 'photo_timeline',
				label: 'Progress photos',
				photos: [
					{ at: '2026-11-01T08:00:00Z', src: '' },
					{ at: '2026-11-08T08:00:00Z', src: '' },
					{ at: '2026-11-15T08:00:00Z', src: '' },
					{ at: '2026-11-22T08:00:00Z', src: '' }
				]
			}
		},
		{ state: 'empty', spec: { kind: 'photo_timeline', label: 'Progress photos', photos: [] } }
	],
	planning: [
		{ state: 'filled', spec: { kind: 'action_list', label: 'Today', actions: today } },
		{ state: 'missed', spec: { kind: 'action_list', label: 'Yesterday', actions: wednesday } },
		{ state: 'empty', spec: { kind: 'action_list', label: 'Saturday', actions: [] } },
		{ state: 'filled', spec: { kind: 'week_strip', label: 'This week', actions: sixpackActions, startDay: '2026-11-23' } },
		{
			state: 'missed',
			spec: {
				kind: 'milestone_timeline',
				label: 'Milestones',
				milestones: [
					{ title: 'First 2 kg down', due: '2026-11-20T00:00:00Z', done: true },
					{ title: 'Body fat under 16 percent', due: '2026-11-25T00:00:00Z', done: false },
					{ title: '80 kg', due: '2026-12-20T00:00:00Z', done: false },
					{ title: '78 kg and visible abs', due: '2027-02-01T00:00:00Z', done: false }
				]
			}
		},
		{
			state: 'filled',
			spec: {
				kind: 'module_checklist',
				label: 'Core technique course',
				modules: [
					{ title: 'Bracing basics', minutes: 20, done: true },
					{ title: 'Anti-rotation work', minutes: 25, done: true },
					{ title: 'Hanging leg raises', minutes: 30, done: false },
					{ title: 'Programming your week', minutes: 45, done: false }
				]
			}
		},
		{ state: 'filled', spec: { kind: 'countdown', label: 'Until Feb 1', deadline: '2027-02-01T00:00:00Z' } },
		{ state: 'missed', spec: { kind: 'countdown', label: 'Until Nov 1', deadline: '2026-11-01T00:00:00Z' } }
	],
	agent: [
		{ state: 'filled', spec: sixpackInbox.approvals[0] },
		{
			state: 'filled',
			spec: {
				kind: 'proposal_card',
				persona: 'Nutritionist',
				summary: 'Swap Sunday meal prep for a Saturday batch cook.',
				changes: [
					{ op: 'create', title: 'Batch cook', before: null, after: 'Sat 28 Nov 11:00 to 12:30' },
					{ op: 'delete', title: 'Meal prep', before: 'Sun 29 Nov 10:00 to 10:30', after: null }
				]
			}
		},
		{ state: 'filled', spec: sixpackInbox.suggestions[0] },
		{ state: 'filled', spec: sixpackInbox.insights[0] },
		{ state: 'empty', spec: weightCheckIn }
	]
};
