import assert from 'node:assert/strict';
import { openingMinute } from './time.ts';

const evening = [{ startMin: 19 * 60, endMin: 20 * 60 }];
const two = [
	{ startMin: 9 * 60, endMin: 10 * 60 },
	{ startMin: 19 * 60, endMin: 20 * 60 }
];

assert.equal(openingMinute([], null), 7 * 60 - 30);
assert.equal(openingMinute(evening, null), 19 * 60 - 30);
assert.equal(openingMinute(two, null), 9 * 60 - 30);
assert.equal(openingMinute([{ startMin: 10, endMin: 40 }], null), 0);

assert.equal(openingMinute([], 15 * 60), 15 * 60 - 30);
assert.equal(openingMinute(evening, 8 * 60), 19 * 60 - 30);
assert.equal(openingMinute(two, 8 * 60), 9 * 60 - 30);
assert.equal(openingMinute(two, 9 * 60 + 30), 9 * 60 - 30);
assert.equal(openingMinute([{ startMin: 9 * 60, endMin: 17 * 60 }], 16 * 60), 15 * 60 - 30);
assert.equal(openingMinute(two, 12 * 60), 19 * 60 - 30);
assert.equal(openingMinute(two, 21 * 60), 21 * 60 - 30);

console.log('time.test.ts ok');
