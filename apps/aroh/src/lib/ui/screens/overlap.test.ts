import { describe, expect, it } from 'vitest';
import { overlapColumns } from './overlap';

describe('overlapColumns (gate 4 D1)', () => {
	it('gives a lone block and blocks that only touch the full width', () => {
		expect(overlapColumns([])).toEqual([]);
		expect(overlapColumns([{ y: 0, h: 48 }])).toEqual([{ col: 0, cols: 1 }]);
		expect(overlapColumns([{ y: 0, h: 48 }, { y: 48, h: 48 }])).toEqual([{ col: 0, cols: 1 }, { col: 0, cols: 1 }]);
	});

	it('splits two overlapping blocks side by side, in input order', () => {
		expect(overlapColumns([{ y: 588, h: 36 }, { y: 576, h: 48 }])).toEqual([{ col: 1, cols: 2 }, { col: 0, cols: 2 }]);
	});

	it('reuses a freed column and keeps one column count per overlap cluster', () => {
		const out = overlapColumns([
			{ y: 0, h: 96 },
			{ y: 24, h: 24 },
			{ y: 60, h: 24 },
			{ y: 200, h: 22 }
		]);
		expect(out).toEqual([{ col: 0, cols: 2 }, { col: 1, cols: 2 }, { col: 1, cols: 2 }, { col: 0, cols: 1 }]);
	});

	it('uses three columns when three blocks share a moment', () => {
		const out = overlapColumns([{ y: 0, h: 60 }, { y: 10, h: 60 }, { y: 20, h: 60 }, { y: 65, h: 10 }]);
		expect(out).toEqual([{ col: 0, cols: 3 }, { col: 1, cols: 3 }, { col: 2, cols: 3 }, { col: 0, cols: 3 }]);
	});
});
