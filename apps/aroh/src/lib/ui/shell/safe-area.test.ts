import { describe, expect, it } from 'vitest';
import { readFileSync } from 'node:fs';
import { join } from 'node:path';

const SRC = join(__dirname, '..', '..', '..');

/** The source of `path` under `src`. */
const source = (path: string) => readFileSync(join(SRC, path), 'utf8');

/** The class attribute of the first tag that starts with `opener` in `path`. */
function classOf(path: string, opener: string): string {
	const text = source(path);
	const at = text.indexOf(opener);
	expect(at, `${opener} in ${path}`).toBeGreaterThanOrEqual(0);
	const found = /class="([^"]*)"/.exec(text.slice(at));
	expect(found, `class of ${opener} in ${path}`).not.toBeNull();
	return found![1];
}

/** Asserts that `classes` pads with the safe-area inset of every edge in `edges`. */
function padsEdges(classes: string, edges: string[]) {
	for (const edge of edges) expect(classes, edge).toContain(`env(safe-area-inset-${edge})`);
}

describe('the app stays clear of the notch, Dynamic Island and home indicator (B124)', () => {
	it('lets the page reach every edge so the insets are known', () => {
		expect(source('app.html')).toMatch(/name="viewport"[^>]*viewport-fit=cover/);
	});

	it('pads the phone shell top bar, body and tab bar', () => {
		const shell = 'lib/ui/shell/PhoneShell.svelte';
		padsEdges(classOf(shell, '<header'), ['top', 'left', 'right']);
		padsEdges(classOf(shell, '<main'), ['left', 'right']);
		padsEdges(classOf(shell, '<nav aria-label="Main"'), ['bottom', 'left', 'right']);
	});

	it('pads the side navigation shell used on iPad and on iPhone in landscape', () => {
		const shell = 'lib/ui/shell/DesktopShell.svelte';
		padsEdges(classOf(shell, '<aside'), ['top', 'bottom', 'left']);
		padsEdges(classOf(shell, '<div class="flex min-w-0 flex-1 flex-col'), ['bottom']);
		padsEdges(classOf(shell, '<header'), ['top', 'right']);
		padsEdges(classOf(shell, '<main'), ['right']);
	});

	it('pads onboarding and every bottom sheet on the sides', () => {
		const files = [
			'lib/ui/screens/OnboardingScreen.svelte',
			'lib/ui/screens/ActionSheet.svelte',
			'lib/ui/screens/InboxScreen.svelte',
			'lib/ui/components/location/RoutineSheet.svelte',
			'lib/ui/components/action/ScopeSheet.svelte',
			'lib/ui/components/goal/DeleteGoalSheet.svelte',
			'lib/ui/components/goal/RunAgainSheet.svelte'
		];
		for (const file of files) padsEdges(source(file), ['left', 'right']);
	});
});
