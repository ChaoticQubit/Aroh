import { render } from 'svelte/server';
import { describe, expect, it } from 'vitest';
import DiaryCard from './DiaryCard.svelte';
import type { DiaryDayView } from '$lib/app/diary';

const day: DiaryDayView = {
	day: '2026-11-26',
	entries: [
		{ id: 'e1', at: '2026-11-26T07:45:00Z', text: 'Slept badly after the call.', source: 'text' },
		{ id: 'e2', at: '2026-11-26T13:10:00Z', text: 'Lunch was rice.', source: 'voice' }
	],
	structure: { summary: 'A tired start.', mood: 2, energy: 3, highlights: ['Same', 'Same'], food: null, water: null, workout: null, notesForTomorrow: null, structuredAt: '2026-11-26T21:21:00Z', editedAt: null },
	done: []
};
const draw = (props: Record<string, unknown>) => render(DiaryCard, { props: { day, zone: 'UTC', ...props } as never }).body;

describe('DiaryCard words, redo and edit (B118, B119, B120)', () => {
	it('b119: your words start collapsed behind an accessible toggle with the count', () => {
		const body = draw({});
		expect(body).toMatch(/<button[^>]*aria-expanded="false"[^>]*aria-controls="[^"]+-words"/);
		expect(body).toContain('Your words · 2 entries');
		expect(body).not.toContain('Slept badly after the call.');
	});

	it('b118 and b120: offers Redo summary and Edit only when the card can change, and renders duplicate highlights', () => {
		const body = draw({ onchange: () => {} });
		expect(body).toContain('Redo summary');
		expect(body).toContain('lucide-pencil');
		expect(body.match(/<li>Same<\/li>/g)).toHaveLength(2);
		const plain = draw({});
		expect(plain).not.toContain('Redo summary');
		expect(plain).not.toContain('lucide-pencil');
	});

	it('b120: an edited card that misses new words says so and names the edit', () => {
		const body = draw({ day: { ...day, stale: true, structure: { ...day.structure!, editedAt: '2026-11-26T22:05:00Z' } }, onchange: () => {} });
		expect(body).toContain('Your newest words are not in this edited card. Redo summary to include them.');
		expect(body).toContain('Edited by you at');
	});

	it('shows a failed redo note next to the card, not only when unstructured', () => {
		expect(draw({ note: 'The engine is busy. Your words are saved.' })).toContain('The engine is busy. Your words are saved.');
	});
});
