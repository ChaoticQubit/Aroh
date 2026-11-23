import { describe, expect, it } from 'vitest';
import { readFileSync, readdirSync, statSync } from 'node:fs';
import { join } from 'node:path';
import { fitKeyboard, type KeyboardWindow } from './keyboard';

/** A window with a visual viewport the test resizes, recording scrolls, the CSS var and the typing flag. */
function fakeWindow() {
	const viewport = new EventTarget() as EventTarget & { height: number };
	viewport.height = 852;
	const page = new EventTarget();
	const vars: Record<string, string> = {};
	const scrolls: [number, number][] = [];
	const flags = new Set<string>();
	const doc = {
		activeElement: null as unknown,
		documentElement: {
			style: { setProperty: (k: string, v: string) => (vars[k] = v) },
			toggleAttribute: (name: string, on: boolean) => (on ? flags.add(name) : flags.delete(name), on)
		}
	};
	const win = {
		visualViewport: viewport,
		addEventListener: page.addEventListener.bind(page),
		removeEventListener: page.removeEventListener.bind(page),
		scrollTo: (x: number, y: number) => scrolls.push([x, y]),
		document: doc
	} as unknown as KeyboardWindow;
	return { win, viewport, page, vars, scrolls, flags, doc };
}

/** A focus event on `target`, as the page sees it. */
function focusEvent(type: 'focusin' | 'focusout', target: unknown): Event {
	const e = new Event(type);
	Object.defineProperty(e, 'target', { value: target });
	return e;
}

/** Every .svelte file under `dir`. */
function svelteFiles(dir: string): string[] {
	return readdirSync(dir).flatMap((name) => {
		const path = join(dir, name);
		return statSync(path).isDirectory() ? svelteFiles(path) : path.endsWith('.svelte') ? [path] : [];
	});
}

describe('phone keyboard and zoom (build 1 feedback)', () => {
	it('fits the app above the keyboard without ever scrolling the page, and hides bars while typing', () => {
		const { win, viewport, page, vars, scrolls, flags, doc } = fakeWindow();
		const stop = fitKeyboard(win);
		expect(vars['--app-h']).toBe('852px');
		let shown: unknown[] = [];
		const field = { matches: (sel: string) => sel.includes('textarea'), scrollIntoView: (o: unknown) => shown.push(o) };
		page.dispatchEvent(focusEvent('focusin', field));
		doc.activeElement = field;
		expect(flags.has('data-typing')).toBe(true);
		viewport.height = 449.4;
		viewport.dispatchEvent(new Event('resize'));
		expect(vars['--app-h']).toBe('449px');
		expect(shown).toEqual([{ block: 'nearest' }]);
		expect(scrolls).toEqual([]);
		const button = { matches: () => false };
		page.dispatchEvent(focusEvent('focusin', button));
		doc.activeElement = button;
		expect(flags.has('data-typing')).toBe(false);
		page.dispatchEvent(focusEvent('focusin', field));
		page.dispatchEvent(focusEvent('focusout', field));
		doc.activeElement = null;
		expect(flags.has('data-typing')).toBe(false);
		stop();
		viewport.height = 852;
		viewport.dispatchEvent(new Event('resize'));
		expect(vars['--app-h']).toBe('449px');
		shown = [];
		page.dispatchEvent(focusEvent('focusin', field));
		expect(flags.has('data-typing')).toBe(false);
	});

	it('hides the tab bar and the pinned Create while typing, so only the field or composer sits on the keyboard', () => {
		const root = join(__dirname, '..', '..');
		const css = readFileSync(join(root, 'app.css'), 'utf8');
		expect(css).toMatch(/\n:root\[data-typing\] \[data-hide-typing\] \{\s*display: none;/);
		const shell = readFileSync(join(root, 'lib/ui/shell/PhoneShell.svelte'), 'utf8');
		expect(shell).toMatch(/<nav aria-label="Main" data-hide-typing/);
		const form = readFileSync(join(root, 'lib/ui/components/goal/NewGoalForm.svelte'), 'utf8');
		expect(form).toMatch(/<div data-hide-typing class="sticky bottom-0/);
	});

	it('never zooms: no pinch zoom, and no text field below 16px', () => {
		const root = join(__dirname, '..', '..');
		expect(readFileSync(join(root, 'app.html'), 'utf8')).toContain('content="width=device-width, initial-scale=1, maximum-scale=1, user-scalable=no, viewport-fit=cover"');
		const small: string[] = [];
		for (const file of svelteFiles(root)) {
			const text = readFileSync(file, 'utf8');
			for (const open of text.matchAll(/<(Input|input|Textarea|textarea|select)\b/g)) {
				let depth = 0;
				let end = open.index! + open[0].length;
				for (; end < text.length; end++) {
					const c = text[end];
					if (c === '{') depth++;
					else if (c === '}') depth--;
					else if (c === '>' && depth === 0) break;
				}
				if (/(?<![:\w-])text-(xs|sm|md|base)\b/.test(text.slice(open.index, end))) small.push(`${file}:${text.slice(0, open.index).split('\n').length}`);
			}
		}
		expect(small).toEqual([]);
	});
});
