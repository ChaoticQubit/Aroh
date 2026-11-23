import type { Card, CardLog, CardMeta } from '$lib/ui/action/card';

/** Leg day, week 2 session 5, written by the coach from week 1's log. */
export const legDay: Card = {
	v: 1,
	depth: 'full',
	why: 'Legs carry most of your strength target. Squats go up 2.5 kg because every set last Thursday felt ok at 60 kg. Lunges build single-leg balance for your half marathon in April.',
	outline: null,
	blocks: [
		{
			kind: 'workout',
			exercises: [
				{ name: 'Back squat', unit: 'kg', rest_s: 120, cue: 'Up 2.5 kg from last week', sets: [{ reps: 8, weight: 62.5 }, { reps: 8, weight: 62.5 }, { reps: 8, weight: 62.5 }, { reps: 8, weight: 62.5 }] },
				{ name: 'Romanian deadlift with a pause at the knee', unit: 'kg', rest_s: 90, cue: null, sets: [{ reps: 10, weight: 50 }, { reps: 10, weight: 50 }, { reps: 10, weight: 50 }] },
				{ name: 'Walking lunges', unit: 'kg', rest_s: 90, cue: 'Dumbbells, total weight', sets: [{ reps: 12, weight: 20 }, { reps: 12, weight: 20 }, { reps: 12, weight: 20 }] },
				{ name: 'Standing calf raises', unit: null, rest_s: 60, cue: null, sets: [{ reps: 15, weight: null }, { reps: 15, weight: null }, { reps: 15, weight: null }] },
				{ name: 'Plank', unit: null, rest_s: 60, cue: 'Seconds, not reps', sets: [{ reps: 45, weight: null }, { reps: 45, weight: null }] }
			]
		},
		{ kind: 'checklist', title: 'Before you start', items: [{ text: '5 min easy bike to warm up' }, { text: 'Water bottle and belt' }] }
	]
};

/** Leg day after the chat edit: lunges swapped for split squats, calf raises dropped, 45 min. */
export const legDayShort: Card = structuredClone(legDay);
{
	const w = legDayShort.blocks[0];
	if (w.kind === 'workout') {
		w.exercises[0] = { ...w.exercises[0], rest_s: 90 };
		w.exercises[2] = { name: 'Bulgarian split squat', unit: 'kg', rest_s: 75, cue: 'Each leg, dumbbells total weight', sets: [{ reps: 8, weight: 16 }, { reps: 8, weight: 16 }, { reps: 8, weight: 16 }] };
		w.exercises.splice(3, 1);
	}
}

export const legDayAction: CardMeta = { title: 'Leg day', goal: 'Get stronger for the half marathon', hue: 2, date: '2026-11-26', start: '18:00', minutes: 60, week: 'Week 2 of 8 · session 5', coach: 'Strength coach' };

/** Mid-workout: squats done (one set at 7 reps), the first RDL set ticked, warm-up checked. */
export const legDayLog: CardLog = {
	sets: [
		{ block: 0, row: 0, set: 0, reps: 8, weight: 62.5, done: true },
		{ block: 0, row: 0, set: 1, reps: 8, weight: 62.5, done: true },
		{ block: 0, row: 0, set: 2, reps: 8, weight: 62.5, done: true },
		{ block: 0, row: 0, set: 3, reps: 7, weight: 62.5, done: true },
		{ block: 0, row: 1, set: 0, reps: 10, weight: 50, done: true }
	],
	checked: [{ block: 1, row: 0 }, { block: 1, row: 1 }],
	effort: null,
	note: null
};

export const emptyLog: CardLog = { sets: [], checked: [], effort: null, note: null };

/** Sunday meal prep: recipe steps and a shopping checklist. */
export const mealPrep: Card = {
	v: 1,
	depth: 'full',
	why: 'Four lunches for Monday to Thursday at 42 g protein each keep you on 140 g a day without cooking on work nights.',
	outline: null,
	blocks: [
		{
			kind: 'checklist',
			title: 'Shopping',
			items: [{ text: 'Chicken thighs, 1 kg' }, { text: 'Jasmine rice, 400 g' }, { text: 'Broccoli, 2 heads' }, { text: 'Greek yogurt, 500 g' }, { text: 'Four 1 litre containers' }]
		},
		{
			kind: 'steps',
			title: 'Recipe',
			items: [
				{ text: 'Rinse the rice and cook it with 800 ml water.', minutes: 15 },
				{ text: 'Season the chicken with salt, paprika and garlic; roast at 220 °C.', minutes: 25 },
				{ text: 'Steam the broccoli while the chicken roasts.', minutes: 6 },
				{ text: 'Mix yogurt, lemon and dill for the sauce.', minutes: 3 },
				{ text: 'Split into four containers: 250 g chicken, 100 g rice, broccoli, sauce on the side.', minutes: 5 }
			]
		}
	]
};

export const mealPrepAction: CardMeta = { title: 'Meal prep: chicken and rice bowls', goal: 'Eat 140 g protein a day', hue: 5, date: '2026-11-29', start: '16:00', minutes: 60, week: 'Week 1 of 12', coach: 'Nutrition coach' };

export const mealPrepLog: CardLog = { sets: [], checked: [{ block: 0, row: 0 }, { block: 0, row: 1 }, { block: 0, row: 2 }], effort: null, note: null };

/** Week 3 Leg day: an outline until the Sunday fill writes it from week 2's logs. */
export const legDayOutline: Card = {
	v: 1,
	depth: 'outline',
	why: '',
	outline: 'Squat 4 x 6 at about 65 kg, Romanian deadlift, split squats, plank. Heavier, fewer reps.',
	blocks: []
};

export const legDayWeek3: CardMeta = { ...legDayAction, date: '2026-12-10', week: 'Week 3 of 8 · session 9' };

/** The coach's one-line outline of each later week of the goal. */
export const laterWeeks: { week: string; dates: string; line: string }[] = [
	{ week: 'Week 4', dates: '14 to 20 Dec', line: 'Lighter week: every lift at 80 %, mobility added.' },
	{ week: 'Week 5', dates: '21 to 27 Dec', line: 'Two sessions over the holidays, no-equipment plan on the 24th.' },
	{ week: 'Week 6', dates: '28 Dec to 3 Jan', line: 'Back to heavy: squat 5 x 5, first test of your 5-rep max.' }
];
