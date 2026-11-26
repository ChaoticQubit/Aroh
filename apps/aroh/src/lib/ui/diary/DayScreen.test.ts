import { render } from 'svelte/server';
import { describe, expect, it, vi } from 'vitest';
import DayScreen from './DayScreen.svelte';
import type { DiaryDayView, DiaryLoad } from '$lib/app/diary';

vi.mock('@tauri-apps/api/core', () => ({ invoke: vi.fn() }));
vi.mock('$lib/ui/notes/PageLoader.svelte', async () => ({ default: (await import('./PageLoaderStub.svelte')).default }));

const day: DiaryDayView = {
	day: '2026-11-26',
	entries: [{ id: 'e1', at: '2026-11-26T07:45:00Z', text: 'Slept badly after the call.', source: 'text' }],
	structure: { summary: 'A tired start.', mood: 2, energy: 3, highlights: [], food: null, water: null, workout: null, notesForTomorrow: null, structuredAt: '2026-11-26T21:21:00Z', editedAt: null },
	done: []
};
const ready: DiaryLoad = { kind: 'ready', today: day, past: [{ ...day, day: '2026-11-25' }], note: null };
const noop = () => {};
const draw = (props: Record<string, unknown>) => render(DayScreen, { props: { day: '2026-11-26', load: ready, zone: 'UTC', current: true, onsend: noop, onopen: noop, onaction: noop, onsettings: noop, onchange: noop, ...props } as never }).body;

describe('DayScreen, the Diary tab as the day page (B126)', () => {
	it('b126: shows the date header, then the diary card with collapsed words, then the day page', () => {
		const body = draw({});
		const header = body.indexOf('Thursday 26 November');
		const card = body.indexOf('aria-label="Diary for');
		const page = body.indexOf('data-testid="page-loader"');
		expect(header).toBeGreaterThan(-1);
		expect(card).toBeGreaterThan(header);
		expect(page).toBeGreaterThan(card);
		expect(body).toContain('Your words · 1 entry');
		expect(body).not.toContain('Slept badly after the call.');
		expect(body).toContain('Redo summary');
		expect(body.indexOf('aria-label="Past days"')).toBeGreaterThan(page);
	});

	it('b126: the day page opens kind day with the date', () => {
		expect(draw({ day: '2026-11-20', current: false, load: { ...ready, today: { ...day, day: '2026-11-20' }, past: [] } })).toContain('page day 2026-11-20');
	});

	it('b126: today has the composer, a past day does not, and a past day without words still shows the page', () => {
		expect(draw({})).toContain('aria-label="Add to diary"');
		const past = draw({ day: '2026-11-20', current: false, load: { ...ready, today: { ...day, day: '2026-11-20', entries: [], structure: null }, past: [] } });
		expect(past).not.toContain('aria-label="Add to diary"');
		expect(past).toContain('No diary for this day on this phone.');
		expect(past).not.toContain('0 diary entries');
		expect(past).toContain('page day 2026-11-20');
	});

	it('b126: off iPhone the diary keeps its unavailable note and the page still renders below it', () => {
		const body = draw({ load: { kind: 'unavailable' } });
		expect(body).toContain('Your diary is available on iPhone');
		expect(body.indexOf('page day 2026-11-26')).toBeGreaterThan(body.indexOf('Your diary is available on iPhone'));
	});
});
