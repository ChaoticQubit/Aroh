import { lockAxis } from '$lib/ui/swipe';

/** How far left a row is dragged before letting go deletes it, in CSS pixels. */
export const DELETE_PX = 96;

/**
 * Svelte action: swipe the row `node` left to delete it. The row follows the finger on a
 * horizontal drag; let go past `DELETE_PX` and `onremove` runs, else it springs back.
 * Vertical drags scroll the page as usual.
 */
export function swipeRemove(node: HTMLElement, onremove: () => void) {
	let start: { x: number; y: number } | null = null;
	let axis: 'x' | 'y' | null = null;
	let dx = 0;
	const place = (x: number) => {
		node.style.transform = x ? `translateX(${x}px)` : '';
		node.style.transition = x ? 'none' : 'transform 150ms ease-out';
	};
	const down = (e: PointerEvent) => {
		if ((e.target as HTMLElement).closest('input, textarea, [data-grip]')) return;
		start = { x: e.clientX, y: e.clientY };
		axis = null;
		dx = 0;
	};
	const move = (e: PointerEvent) => {
		if (!start) return;
		axis ??= lockAxis(e.clientX - start.x, e.clientY - start.y);
		if (axis !== 'x') return;
		dx = Math.min(0, e.clientX - start.x);
		place(dx);
	};
	const up = () => {
		if (start && axis === 'x' && dx <= -DELETE_PX) onremove();
		start = null;
		place(0);
	};
	node.addEventListener('pointerdown', down);
	node.addEventListener('pointermove', move);
	node.addEventListener('pointerup', up);
	node.addEventListener('pointercancel', up);
	return {
		update(next: () => void) {
			onremove = next;
		},
		destroy() {
			node.removeEventListener('pointerdown', down);
			node.removeEventListener('pointermove', move);
			node.removeEventListener('pointerup', up);
			node.removeEventListener('pointercancel', up);
		}
	};
}

/**
 * Rows moved by a grip drag of `dy` pixels over rows `height` pixels tall, rounded to the
 * nearest row. Returns a whole number, negative for up.
 */
export function dragSteps(dy: number, height: number): number {
	console.assert(height > 0, 'a row has a height');
	return Math.round(dy / height);
}

/**
 * Svelte action for the grip of row `index`: drag it up or down to move the row, or press
 * ArrowUp or ArrowDown on it. Calls `onmove(from, to)` once per gesture or key press.
 */
export function grip(node: HTMLElement, params: { index: number; onmove: (from: number, to: number) => void }) {
	let p = params;
	let start: number | null = null;
	const row = () => node.closest('[data-row]') as HTMLElement | null;
	const down = (e: PointerEvent) => {
		start = e.clientY;
		node.setPointerCapture(e.pointerId);
		e.preventDefault();
	};
	const move = (e: PointerEvent) => {
		const r = row();
		if (start !== null && r) r.style.transform = `translateY(${e.clientY - start}px)`;
	};
	const up = (e: PointerEvent) => {
		const r = row();
		if (start !== null && r) {
			const steps = dragSteps(e.clientY - start, r.getBoundingClientRect().height || 1);
			r.style.transform = '';
			if (steps) p.onmove(p.index, p.index + steps);
		}
		start = null;
	};
	const key = (e: KeyboardEvent) => {
		if (e.key === 'ArrowUp' || e.key === 'ArrowDown') {
			e.preventDefault();
			p.onmove(p.index, p.index + (e.key === 'ArrowUp' ? -1 : 1));
		}
	};
	node.addEventListener('pointerdown', down);
	node.addEventListener('pointermove', move);
	node.addEventListener('pointerup', up);
	node.addEventListener('pointercancel', up);
	node.addEventListener('keydown', key);
	return {
		update(next: typeof params) {
			p = next;
		},
		destroy() {
			node.removeEventListener('pointerdown', down);
			node.removeEventListener('pointermove', move);
			node.removeEventListener('pointerup', up);
			node.removeEventListener('pointercancel', up);
			node.removeEventListener('keydown', key);
		}
	};
}
