/** One goal on the dashboard: identity, plan progress to date, projection and its tracker trend. */
export type DashGoal = {
	id: string;
	title: string;
	category: string;
	cat: number;
	series: number;
	done: number;
	planned: number;
	finish: string;
	target: string;
	streak: number;
	week: number;
	month: number;
	minutes: number[];
	tracker: { label: string; unit: string; points: number[]; target?: number };
};

/** Mondays of the six weeks the minutes chart shows, oldest first. */
export const weekStarts = ['2026-10-19', '2026-10-26', '2026-11-02', '2026-11-09', '2026-11-16', '2026-11-23'];

/** The sample goals, in fixed series order so a filter never repaints a goal. */
export const goals: DashGoal[] = [
	{
		id: 'g1',
		title: 'Finish reading Harbor Lights',
		category: 'Learning',
		cat: 1,
		series: 3,
		done: 5,
		planned: 5,
		finish: '2026-12-15',
		target: '2026-12-31',
		streak: 5,
		week: 100,
		month: 92,
		minutes: [0, 0, 60, 100, 120, 80],
		tracker: {
			label: 'Chapters read',
			unit: '',
			points: [0, 1, 1, 2, 3, 4, 5],
			target: 24
		}
	},
	{
		id: 'g2',
		title: 'Learn Spanish basics',
		category: 'Learning',
		cat: 1,
		series: 4,
		done: 18,
		planned: 24,
		finish: '2027-02-20',
		target: '2027-01-31',
		streak: 2,
		week: 60,
		month: 71,
		minutes: [90, 75, 60, 60, 45, 30],
		tracker: {
			label: 'Words learned',
			unit: '',
			points: [40, 85, 120, 150, 170, 185, 192],
			target: 500
		}
	},
	{
		id: 'g3',
		title: 'Six pack by Feb 1',
		category: 'Fitness',
		cat: 5,
		series: 5,
		done: 22,
		planned: 24,
		finish: '2027-01-28',
		target: '2027-02-01',
		streak: 6,
		week: 100,
		month: 90,
		minutes: [120, 135, 150, 180, 180, 120],
		tracker: {
			label: 'Plank hold',
			unit: 's',
			points: [45, 50, 60, 65, 75, 80, 90],
			target: 120
		}
	},
	{
		id: 'g4',
		title: 'Meditate daily',
		category: 'Mindfulness',
		cat: 4,
		series: 6,
		done: 26,
		planned: 35,
		finish: '2026-12-20',
		target: '2026-12-13',
		streak: 0,
		week: 43,
		month: 74,
		minutes: [70, 70, 60, 50, 40, 20],
		tracker: {
			label: 'Minutes a day',
			unit: 'min',
			points: [10, 10, 10, 8, 6, 5, 5],
			target: 10
		}
	}
];

/** Whether a goal is on track: done keeps up with planned to date and the projected finish is on or before the target. */
export function onTrack(g: DashGoal): boolean {
	return g.done / g.planned >= 0.85 && g.finish <= g.target;
}
