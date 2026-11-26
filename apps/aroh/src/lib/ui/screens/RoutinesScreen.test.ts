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

describe('RoutinesScreen opens, edits and deletes routines (B110)', () => {
	const goal = { id: 'r', title: 'Routines', category: 'Habit', slot: 1, deadline: null, autonomy: 'Ask', lead: null };
	const places = {
		routines: [
			{ series_id: 's1', title: 'Leg day', place: 'Iron Works Gym', cadence: 'Weekly, Mon', streak: '2 weeks', last_done: null, next: { action_id: 'a1', start_at: '2026-11-30T06:00:00Z', status: 'planned' } },
			{ series_id: 's2', title: 'Swim', place: null, cadence: 'Weekly, Fri', streak: '0 weeks', last_done: null, next: null }
		]
	} as unknown as LocationView;
	const routines = routineRows(places, '2026-11-26T12:00:00Z', 'UTC');
	const draw = (props: Record<string, unknown>) => render(RoutinesScreen, { props: { goal, view: { title: 'Routines', sections: [] }, routines, inline: true, ...props } as never }).body;

	it('b110: every row is a button and Routines has a Delete Routines action', () => {
		const body = draw({});
		expect(body).toContain('aria-label="Open Leg day"');
		expect(body).toContain('aria-label="Open Swim"');
		expect(body).toMatch(/>\s*Delete Routines\s*</);
		expect(body).not.toContain('role="dialog"');
	});

	it('b110: a tapped row opens its sheet with Edit on the next session and Delete', () => {
		const body = draw({ opened: 's1' });
		expect(body).toContain('role="dialog"');
		expect(body).toContain('href="/actions/a1"');
		expect(body).toContain('Edit next session');
		expect(body).toMatch(/>\s*Delete\s*</);
	});

	it('b110: a routine with no session planned has no Edit but can still be deleted', () => {
		const body = draw({ opened: 's2' });
		expect(body).toContain('No session planned yet');
		expect(body).not.toContain('Edit next session');
		expect(body).toMatch(/>\s*Delete\s*</);
	});

	it('b110: deleting one routine asks once and keeps done and logged sessions', () => {
		const body = draw({ opened: 's1', confirming: true });
		expect(body).toContain('role="alertdialog"');
		expect(body).toContain('from now?');
		expect(body).toContain('Done and logged sessions stay.');
		expect(body).toMatch(/>\s*Delete routine\s*</);
	});

	it('b110: Delete Routines confirms ending every routine', () => {
		const body = draw({ opened: 'routines' });
		expect(body).toContain('role="alertdialog"');
		expect(body).toContain('Every routine ends from now');
		expect(body).toMatch(/>\s*Delete Routines\s*</);
	});
});
