import { afterEach, describe, expect, it } from 'vitest';
import { deviceZone, setDeviceZone } from './clock';

describe('device zone', () => {
	afterEach(() => setDeviceZone(null));

	it('uses the zone the store follows when the snapshot has one', () => {
		const web = Intl.DateTimeFormat().resolvedOptions().timeZone;
		expect(deviceZone()).toBe(web);
		setDeviceZone('Asia/Tokyo');
		expect(deviceZone()).toBe('Asia/Tokyo');
		setDeviceZone(undefined);
		expect(deviceZone()).toBe(web);
	});
});
