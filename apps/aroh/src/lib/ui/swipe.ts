import { shiftDay, shiftMonth } from './format';

/** Calendar views a swipe can page through. */
export type CalendarView = 'day' | 'week' | 'month';

const LOCK_PX = 10;
const EDGE_PX = 24;
const FLICK_PX_PER_MS = 0.5;
const SNAP_MS = 220;

/**
 * The anchor day one page from `anchor` (YYYY-MM-DD) in `dir`: a day, a week, or the first of
 * the next or previous month. Returns the new anchor as YYYY-MM-DD.
 */
export function stepAnchor(view: CalendarView, anchor: string, dir: 1 | -1): string {
	if (view === 'month') return shiftMonth(anchor, dir);
	return shiftDay(anchor, view === 'day' ? dir : dir * 7);
}

/**
 * The axis a touch moved along once it travelled `LOCK_PX` (`dx`, `dy` from the start in px):
 * 'x' only when clearly sideways, 'y' otherwise, null while it is still jitter.
 */
export function lockAxis(dx: number, dy: number): 'x' | 'y' | null {
	if (Math.hypot(dx, dy) < LOCK_PX) return null;
	return Math.abs(dx) > Math.abs(dy) * 1.2 ? 'x' : 'y';
}

/**
 * The page step for a released sideways swipe: 1 (next) for a left swipe, -1 for a right one, 0
 * to snap back. It pages past a quarter of `width` px, or on a flick of `velocity` px/ms that
 * goes the same way as `dx`.
 */
export function swipeStep(dx: number, width: number, velocity: number): -1 | 0 | 1 {
	console.assert(width > 0, 'a swipe area has a width');
	const far = Math.abs(dx) > width / 4;
	const flick = Math.abs(velocity) > FLICK_PX_PER_MS && Math.sign(velocity) === Math.sign(dx);
	if (!far && !flick) return 0;
	return dx < 0 ? 1 : -1;
}

/**
 * Svelte action: the node follows a sideways finger and snaps; on a page step it slides out,
 * calls `onstep(dir)`, then slides the new page in. Touches that start at the screen edges (the
 * iOS back swipe), go vertical (scroll), or start in `[data-no-swipe]` or a sideways scroller
 * are left alone.
 */
export function swipe(node: HTMLElement, onstep: (dir: 1 | -1) => void) {
	let current = onstep;
	let start: { x: number; y: number; t: number } | null = null;
	let axis: 'x' | 'y' | null = null;
	let last = { x: 0, t: 0, v: 0 };

	const move = (x: number, ms: number) => {
		if (node.parentElement) node.parentElement.style.overflowX = 'clip';
		node.style.transition = ms ? `transform ${ms}ms ease-out` : 'none';
		node.style.transform = x ? `translate3d(${x}px, 0, 0)` : '';
	};
	const settle = () => {
		move(0, SNAP_MS);
		setTimeout(() => node.parentElement?.style.removeProperty('overflow-x'), SNAP_MS);
	};
	const begin = (e: TouchEvent) => {
		const t = e.touches[0];
		const target = e.target as Element | null;
		const edge = t.clientX < EDGE_PX || t.clientX > innerWidth - EDGE_PX;
		start = e.touches.length === 1 && !edge && !target?.closest('[data-no-swipe], .overflow-x-auto') ? { x: t.clientX, y: t.clientY, t: e.timeStamp } : null;
		axis = null;
		last = { x: t.clientX, t: e.timeStamp, v: 0 };
	};
	const track = (e: TouchEvent) => {
		if (!start) return;
		const t = e.touches[0];
		const dx = t.clientX - start.x;
		axis ??= lockAxis(dx, t.clientY - start.y);
		if (axis === 'x' && node.querySelector('[data-dragging]')) axis = 'y';
		if (axis !== 'x') return;
		e.preventDefault();
		const dt = Math.max(1, e.timeStamp - last.t);
		last = { x: t.clientX, t: e.timeStamp, v: (t.clientX - last.x) / dt };
		move(dx, 0);
	};
	const end = (e: TouchEvent) => {
		if (!start || axis !== 'x') return void (start = null);
		const dx = (e.changedTouches[0]?.clientX ?? last.x) - start.x;
		const width = node.clientWidth || innerWidth;
		const dir = swipeStep(dx, width, last.v);
		start = null;
		if (!dir) return settle();
		move(-dir * width, SNAP_MS);
		setTimeout(() => {
			current(dir);
			move(dir * width, 0);
			requestAnimationFrame(() => requestAnimationFrame(settle));
		}, SNAP_MS);
	};

	node.style.touchAction = 'pan-y';
	node.addEventListener('touchstart', begin, { passive: true });
	node.addEventListener('touchmove', track, { passive: false });
	node.addEventListener('touchend', end);
	node.addEventListener('touchcancel', end);
	return {
		update(next: (dir: 1 | -1) => void) {
			current = next;
		},
		destroy() {
			node.removeEventListener('touchstart', begin);
			node.removeEventListener('touchmove', track);
			node.removeEventListener('touchend', end);
			node.removeEventListener('touchcancel', end);
		}
	};
}
