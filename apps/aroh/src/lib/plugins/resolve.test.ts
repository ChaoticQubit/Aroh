import assert from 'node:assert/strict';
import { actionRow, fraction, listProp, numberProp, showNumber } from './resolve.ts';

const data = { 'water.today_total': 750, 'today.actions': [{ id: 'a', title: 'Read', status: 'done' }], 'bad.num': Number.NaN };

assert.equal(numberProp({ query: 'water.today_total' }, data), 750);
assert.equal(numberProp({ query: 'missing' }, data), null);
assert.equal(numberProp({ query: 'today.actions' }, data), null);
assert.equal(numberProp({ query: 'bad.num' }, data), null);
assert.equal(numberProp(2500, data), 2500);
assert.equal(numberProp('2500', data), null);
assert.equal(listProp({ query: 'today.actions' }, data)?.length, 1);
assert.equal(listProp({ query: 'water.today_total' }, data), null);

assert.equal(fraction(750, 2500), 0.3);
assert.equal(fraction(3000, 2500), 1);
assert.equal(fraction(-5, 10), 0);
assert.equal(fraction(5, 0), 0);
assert.equal(fraction(null, 10), 0);
assert.equal(showNumber(null), '-');

assert.deepEqual(actionRow({ id: 'a', title: 'Read', status: 'done' }, 0), { id: 'a', title: 'Read', startAt: null, status: 'done', done: true });
assert.equal(actionRow({ title: '' }, 0), null);
assert.equal(actionRow('x', 0), null);
assert.equal(actionRow({ title: 'T' }, 3)?.id, 'row-3');

console.log('resolve.test.ts: ok');
