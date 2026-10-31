import { describe, it, expect } from 'vitest';
import { parseSortKey, serializeSortKey, sortKeysToSlot, sortSlotToKeys } from './sortSlot';

function counter(): () => number {
	let next = 0;
	return () => (next += 1);
}

describe('parseSortKey', () => {
	it('reads a bare field as ascending', () => {
		expect(parseSortKey('created', 1)).toEqual({ localId: 1, field: 'created', direction: 'asc' });
	});

	it('reads an explicit direction', () => {
		expect(parseSortKey('created:desc', 1).direction).toBe('desc');
		expect(parseSortKey('created:asc', 1).direction).toBe('asc');
	});

	it('ignores whitespace around the parts', () => {
		expect(parseSortKey(' priority : desc ', 1)).toEqual({
			localId: 1,
			field: 'priority',
			direction: 'desc'
		});
	});
});

describe('sortSlotToKeys', () => {
	it('seeds one row from a string and one per entry from a list', () => {
		expect(sortSlotToKeys('created:desc', counter())).toHaveLength(1);
		const rows = sortSlotToKeys(['priority:desc', 'created'], counter());
		expect(rows.map((row) => row.field)).toEqual(['priority', 'created']);
		expect(rows.map((row) => row.localId)).toEqual([1, 2]);
	});

	it('seeds nothing for an absent or malformed slot', () => {
		expect(sortSlotToKeys(undefined, counter())).toEqual([]);
		expect(sortSlotToKeys(123, counter())).toEqual([]);
	});
});

describe('sortKeysToSlot', () => {
	it('writes one key as a string and several as a list', () => {
		expect(sortKeysToSlot([{ localId: 1, field: 'created', direction: 'desc' }])).toBe(
			'created:desc'
		);
		expect(
			sortKeysToSlot([
				{ localId: 1, field: 'priority', direction: 'desc' },
				{ localId: 2, field: 'created', direction: 'asc' }
			])
		).toEqual(['priority:desc', 'created']);
	});

	it('skips rows without a field and is undefined when none remain', () => {
		expect(sortKeysToSlot([{ localId: 1, field: '', direction: 'asc' }])).toBeUndefined();
		expect(sortKeysToSlot([])).toBeUndefined();
	});

	it('round-trips what it parsed', () => {
		const slot = ['priority:desc', 'created'];
		expect(sortKeysToSlot(sortSlotToKeys(slot, counter()))).toEqual(slot);
		expect(serializeSortKey(parseSortKey('status', 1))).toBe('status');
	});
});
