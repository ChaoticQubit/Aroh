import { engine, type Engine } from './engine';
import { originRun, pickRun } from './goalcard';

const GOAL_PATH = /^\/goals\/([^/?#]+)/;

/**
 * Starts the engine reads the goal page of `goalId` makes on open (its runs, the persona library,
 * then its picked and origin run views), so the cache is warm before the page renders (B87).
 * Reads already in flight are shared, so a tap followed by the page open sends each read once.
 */
export async function prefetchGoal(goalId: string, e: Pick<Engine, 'runs' | 'run' | 'personas'> = engine): Promise<void> {
	console.assert(goalId !== '', 'a prefetch names a goal');
	const [runs] = await Promise.all([e.runs({ goalId }), e.personas()]);
	if (!runs.ok) return;
	const ids = new Set([pickRun(runs.value), originRun(runs.value)].filter((id): id is string => id !== null));
	await Promise.all([...ids].map((id) => e.run(id)));
}

/**
 * The goal a tapped element opens: its `data-goal` id, else the id in its `/goals/<id>` link.
 * Null for anything else, the new-goal form included.
 */
export function goalTarget(dataGoal: string | null, href: string | null): string | null {
	if (dataGoal) return dataGoal;
	const id = href ? GOAL_PATH.exec(href)?.[1] : undefined;
	return id && id !== 'new' ? id : null;
}

/** A pointerdown anywhere in the app: prefetches the goal the pressed element opens, if any. */
export function prefetchOnPress(event: PointerEvent) {
	const el = event.target instanceof Element ? event.target.closest('[data-goal], a[href^="/goals/"]') : null;
	const id = el ? goalTarget(el.getAttribute('data-goal'), el.getAttribute('href')) : null;
	if (id) void prefetchGoal(id);
}
