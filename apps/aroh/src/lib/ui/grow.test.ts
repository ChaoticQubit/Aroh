import { describe, expect, it } from 'vitest';
import { GROW_LINES, grownHeight } from './grow';

describe('composer auto-grow (B61)', () => {
	it('grows with the text up to six lines, then scrolls inside', () => {
		const line = 28;
		const pad = 18;
		expect(grownHeight(line + pad, line, pad)).toEqual({ height: line + pad, scroll: false });
		expect(grownHeight(3 * line + pad, line, pad)).toEqual({ height: 3 * line + pad, scroll: false });
		expect(grownHeight(GROW_LINES * line + pad, line, pad)).toEqual({ height: GROW_LINES * line + pad, scroll: false });
		expect(grownHeight(20 * line + pad, line, pad)).toEqual({ height: GROW_LINES * line + pad, scroll: true });
	});

	it('never shrinks below one line', () => {
		expect(grownHeight(0, 28, 18)).toEqual({ height: 46, scroll: false });
	});
});
