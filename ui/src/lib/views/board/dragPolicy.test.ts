import { describe, expect, it } from 'vitest';

import { boardDragDisabledReason, boardDragEnabled } from './dragPolicy';

describe('boardDragEnabled', () => {
	it('allows dragging on single-valued grouping fields', () => {
		expect(boardDragEnabled('choice')).toBe(true);
		expect(boardDragEnabled('string')).toBe(true);
	});

	it('blocks dragging on a multichoice board', () => {
		// A multichoice card is in several columns at once, so a drop has
		// no unambiguous meaning — and the write it used to attempt
		// discarded every other value.
		expect(boardDragEnabled('multichoice')).toBe(false);
	});
});

describe('boardDragDisabledReason', () => {
	it('explains the multichoice case', () => {
		expect(boardDragDisabledReason('multichoice')).toContain('multi-value');
	});

	it('has nothing to say when dragging works', () => {
		expect(boardDragDisabledReason('choice')).toBeNull();
	});
});
