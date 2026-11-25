import { readFileSync } from 'node:fs';
import { render } from 'svelte/server';
import { describe, expect, it } from 'vitest';
import Logo from './Logo.svelte';

const source = readFileSync(new URL('./aroh-icon.svg', import.meta.url), 'utf8');

describe('Logo', () => {
	it('names the mark for assistive tech by default', () => {
		const { body } = render(Logo, { props: { size: 28 } });
		expect(body).toMatch(/<img[^>]+alt="Aroh"/);
		expect(body).not.toContain('aria-hidden');
		expect(body).toContain('width="28"');
	});

	it('is decorative with an empty label', () => {
		const { body } = render(Logo, { props: { label: '' } });
		expect(body).toContain('alt=""');
		expect(body).toContain('aria-hidden="true"');
	});

	it('draws the same dark tile in light and dark theme', () => {
		const { body } = render(Logo, {});
		expect(body).not.toMatch(/dark:|var\(--/);
		expect(source).toContain('<rect width="100" height="100" fill="#16181C"/>');
		expect(source).toContain('stroke="#ECE8E1"');
		expect(source).toContain('fill="#D9A441"');
		expect(source).not.toMatch(/currentColor|prefers-color-scheme/);
	});
});
