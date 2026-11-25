const UUID = /^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$/i;

/** A tapped server push as the app's Rust side hands it over: kind, row id and goal id. */
export type PushTap = { kind: string; id: string; goal_id?: string | null };

/**
 * Where a tapped push opens (Q83): a run's goal chat, the Inbox for a run without a goal, a
 * failed run's goal page (Q88, the Inbox without a goal), or a proposal's goal. The payload crosses a trust boundary, so any other kind, shape or a non-uuid
 * id gives null and the app stays where it is.
 */
export function pushTarget(tap: unknown): string | null {
	if (typeof tap !== 'object' || tap === null) return null;
	const { kind, id, goal_id: goal = null } = tap as Record<string, unknown>;
	if (typeof id !== 'string' || !UUID.test(id)) return null;
	if (goal !== null && (typeof goal !== 'string' || !UUID.test(goal))) return null;
	if (kind === 'run') return goal === null ? '/inbox' : `/goals/${goal}/chat`;
	if (kind === 'run_failed') return goal === null ? '/inbox' : `/goals/${goal}`;
	if (kind === 'proposal' && goal !== null) return `/goals/${goal}`;
	return null;
}
