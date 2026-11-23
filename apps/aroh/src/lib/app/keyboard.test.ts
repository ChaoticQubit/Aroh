import { describe, expect, it } from 'vitest';
import { readFileSync, readdirSync, statSync } from 'node:fs';
import { join } from 'node:path';
import { fitKeyboard, type KeyboardWindow } from './keyboard';

/** A window with a visual viewport the test resizes, recording scrolls, timers and the CSS var. */
function fakeWindow() {
	const viewport = new EventTarget() as EventTarget & { height: number };
	viewport.height = 852;
	const page = new EventTarget();
	const vars: Record<string, string> = {};
	const scrolls: [number, number][] = [];
	const timers: (() => void)[] = [];
	const win = {
		visualViewport: viewport,
		addEventListener: page.addEventListener.bind(page),
		removeEventListener: page.removeEventListener.bind(page),
		scrollTo: (x: number, y: number) => scrolls.push([x, y]),
		setTimeout: (f: () => void) => timers.push(f),
		document: { documentElement: { style: { setProperty: (k: string, v: string) => (vars[k] = v) } } }
	} as unknown as KeyboardWindow;
	return { win, viewport, page, vars, scrolls, timers };
}

/** Every .svelte file under `dir`. */
function svelteFiles(dir: string): string[] {
	return readdirSync(dir).flatMap((name) => {
		const path = join(dir, name);
		return statSync(path).isDirectory() ? svelteFiles(path) : path.endsWith('.svelte') ? [path] : [];
	});
}

describe('phone keyboard and zoom (build 1 feedback)', () => {
	it('fits the app to the space above the keyboard and brings the focused field into view', () => {
		const { win, viewport, page, vars, scrolls, timers } = fakeWindow();
		const stop = fitKeyboard(win);
		expect(vars['--app-h']).toBe('852px');
		viewport.height = 516.4;
		viewport.dispatchEvent(new Event('resize'));
		expect(vars['--app-h']).toBe('516px');
		expect(scrolls.at(-1)).toEqual([0, 0]);
		let shown = 0;
		const field = { matches: (s: string) => s.includes('textarea'), scrollIntoView: () => shown++ };
		const focus = new Event('focusin');
		Object.defineProperty(focus, 'target', { value: field });
		page.dispatchEvent(focus);
		timers.forEach((t) => t());
		expect(shown).toBe(1);
		stop();
		viewport.height = 852;
		viewport.dispatchEvent(new Event('resize'));
		expect(vars['--app-h']).toBe('516px');
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
