import { describe, expect, it } from 'vitest';
import { pushTarget } from './push';

const RUN = '0190f2a0-0000-7000-8000-000000000009';
const GOAL = '0190f2a0-0000-7000-8000-00000000000a';

describe('pushTarget', () => {
	it('opens the goal chat for a run and the goal for a proposal', () => {
		expect(pushTarget({ kind: 'run', id: RUN, goal_id: GOAL })).toBe(`/goals/${GOAL}/chat`);
		expect(pushTarget({ kind: 'proposal', id: RUN, goal_id: GOAL })).toBe(`/goals/${GOAL}`);
	});

	it('opens the Inbox for a run without a goal', () => {
		expect(pushTarget({ kind: 'run', id: RUN, goal_id: null })).toBe('/inbox');
		expect(pushTarget({ kind: 'run', id: RUN })).toBe('/inbox');
	});

	it('refuses anything the payload should not carry', () => {
		expect(pushTarget(null)).toBeNull();
		expect(pushTarget('run')).toBeNull();
		expect(pushTarget({ kind: 'proposal', id: RUN, goal_id: null })).toBeNull();
		expect(pushTarget({ kind: 'reminder', id: RUN, goal_id: GOAL })).toBeNull();
		expect(pushTarget({ kind: 'run', id: 'not-a-uuid', goal_id: GOAL })).toBeNull();
		expect(pushTarget({ kind: 'run', id: RUN, goal_id: '../settings' })).toBeNull();
		expect(pushTarget({ kind: 'run', id: RUN, goal_id: `${GOAL}/x` })).toBeNull();
		expect(pushTarget({ kind: 'run', id: RUN, goal_id: 7 })).toBeNull();
	});
});
