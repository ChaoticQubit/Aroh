import { render } from 'svelte/server';
import { describe, expect, it } from 'vitest';
import RoutinesScreen from './RoutinesScreen.svelte';
import { routineRows, type LocationView } from '$lib/app/location';

describe('RoutinesScreen (B96)', () => {
	it('b96: renders a routine whose occurrences were all deleted, with no next day', () => {
		const places = { routines: [{ series_id: 's1', title: 'Leg day', place: null, cadence: 'Weekly, Mon', streak: '0 weeks', last_done: null, next: null }] } as unknown as LocationView;
		const routines = routineRows(places, '2026-11-25T12:00:00Z', 'UTC');
		const goal = { id: 'r', title: 'Routines', category: 'Habit', slot: 1, deadline: null, autonomy: 'Ask', lead: null };
		const { body } = render(RoutinesScreen, { props: { goal, view: { title: 'Routines', sections: [] }, routines } as never });
		expect(body).toContain('Leg day');
		expect(body).toContain('Not planned');
	});
});
