import { describe, expect, it } from 'vitest';
import { markOf } from './itemMark';

describe('markOf', () => {
	it('marks the open item', () => {
		expect(markOf('a', 'a', null)).toBe('open');
	});

	it('marks the last opened item once nothing is open', () => {
		expect(markOf('a', null, 'a')).toBe('last');
	});

	it('lets the open mark win over the last mark for the same item', () => {
		expect(markOf('a', 'a', 'a')).toBe('open');
	});

	it('marks the previous item as last while another is open', () => {
		expect(markOf('a', 'b', 'a')).toBe('last');
	});

	it('marks nothing for an item that is neither', () => {
		expect(markOf('c', 'a', 'b')).toBe(null);
		expect(markOf('c', null, null)).toBe(null);
	});
});
