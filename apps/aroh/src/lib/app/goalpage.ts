/** A value the goal page read for one goal, tagged with that goal's id (B87). */
export type Tagged<T> = { id: string; value: T };

/** Tags `value` as read for goal `id`. */
export function tagged<T>(id: string, value: T): Tagged<T> {
	console.assert(id !== '', 'a goal value is tagged with a goal id');
	return { id, value };
}

/**
 * The value of `held` when it was read for goal `id`, else `empty`. The page is reused across
 * goal ids, so a value of the previous goal, or a late answer for it, never shows on this one.
 */
export function forGoal<T>(held: Tagged<T> | null, id: string | undefined, empty: T): T {
	return held && id !== undefined && held.id === id ? held.value : empty;
}

/**
 * The goal id a goal page may read the engine for: `id` once `snapshot` holds that live goal,
 * else null. A deleted or unknown goal, or one not loaded yet, gets no engine read (B96).
 */
export function openGoalId(snapshot: { goals: { id: string }[] } | null | undefined, id: string | undefined): string | null {
	if (!snapshot || !id) return null;
	return snapshot.goals.some((g) => g.id === id) ? id : null;
}
