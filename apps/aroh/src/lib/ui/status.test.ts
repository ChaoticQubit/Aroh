import { describe, expect, it } from 'vitest';
import { actionStyle } from './status';

describe('action status style', () => {
	it('draws done struck with a green Done label and no category tint, the same everywhere', () => {
		expect(actionStyle('done')).toEqual({ title: 'text-muted line-through', label: 'Done', labelClass: 'text-status-done', tint: 0 });
		expect(actionStyle('planned')).toEqual({ title: 'text-fg', label: null, labelClass: '', tint: 8 });
	});

	it('keeps missed red and struck, skipped grey, both untinted', () => {
		expect(actionStyle('missed')).toMatchObject({ title: 'text-status-missed line-through', label: 'Missed', labelClass: 'text-status-missed', tint: 0 });
		expect(actionStyle('skipped')).toMatchObject({ title: 'text-muted line-through', label: 'Skipped', labelClass: 'text-status-skipped', tint: 0 });
	});
});
