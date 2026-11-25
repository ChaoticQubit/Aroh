import { render } from 'svelte/server';
import { describe, expect, it } from 'vitest';
import ErrorPage from '../../routes/+error.svelte';

describe('app error page (B96)', () => {
	it('b96: says something went wrong with a way back, never a status code or "internal"', () => {
		const { body } = render(ErrorPage);
		expect(body).toContain('Something went wrong on this screen.');
		expect(body).toMatch(/href="\/goals"/);
		expect(body).not.toMatch(/\d{3}|internal/i);
	});
});
