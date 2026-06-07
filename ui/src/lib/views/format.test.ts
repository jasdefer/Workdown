import { describe, it, expect } from 'vitest';
import { formatIsoDate, formatScalar, pluralize } from './format';

describe('formatIsoDate', () => {
	it('formats a date as YYYY-MM-DD', () => {
		// Construct via local-time parts so the assertion is timezone-stable.
		expect(formatIsoDate(new Date(2026, 5, 3))).toBe('2026-06-03');
	});

	it('zero-pads single-digit month and day', () => {
		expect(formatIsoDate(new Date(2026, 0, 9))).toBe('2026-01-09');
	});

	it('handles December (month index 11)', () => {
		expect(formatIsoDate(new Date(2025, 11, 31))).toBe('2025-12-31');
	});
});

describe('pluralize', () => {
	it('uses the singular noun for a count of one', () => {
		expect(pluralize(1, 'item')).toBe('1 item');
	});

	it('adds "s" for counts other than one', () => {
		expect(pluralize(0, 'item')).toBe('0 items');
		expect(pluralize(5, 'bar')).toBe('5 bars');
		expect(pluralize(7, 'working day')).toBe('7 working days');
	});

	it('uses an explicit plural when given', () => {
		expect(pluralize(2, 'entry', 'entries')).toBe('2 entries');
	});
});

describe('formatScalar', () => {
	it('renders durations as suffix shorthand', () => {
		expect(formatScalar(219600, true)).toBe('2d 13h');
	});

	it('renders plain numbers via formatNumber', () => {
		expect(formatScalar(42, false)).toBe('42');
		expect(formatScalar(2.5, false)).toBe('2.5');
	});
});
