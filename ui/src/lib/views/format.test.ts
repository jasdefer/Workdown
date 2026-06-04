import { describe, it, expect } from 'vitest';
import { formatIsoDate } from './format';

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
