import { describe, it, expect } from 'vitest';
import {
	anchoredElapsedSeconds,
	formatClock,
	projectedNewSeconds,
	roundedWriteSeconds
} from './timerMath';

// The same boundary table as the Rust side (`timer_data.rs`) — the two
// implementations must never disagree.
describe('roundedWriteSeconds', () => {
	it('writes nothing under half a minute', () => {
		expect(roundedWriteSeconds(0)).toBe(0);
		expect(roundedWriteSeconds(29)).toBe(0);
	});

	it('rounds thirty seconds up to one minute', () => {
		expect(roundedWriteSeconds(30)).toBe(60);
		expect(roundedWriteSeconds(89)).toBe(60);
	});

	it('rounds ninety seconds up to two minutes', () => {
		expect(roundedWriteSeconds(90)).toBe(120);
	});

	it('keeps exact minutes exact', () => {
		expect(roundedWriteSeconds(60)).toBe(60);
		expect(roundedWriteSeconds(3600)).toBe(3600);
	});
});

describe('projectedNewSeconds', () => {
	it('starts an absent effort field from zero', () => {
		expect(projectedNewSeconds(null, 120)).toBe(120);
	});

	it('adds the rounded elapsed to the existing value', () => {
		expect(projectedNewSeconds(7200, 2520)).toBe(7200 + 2520);
	});
});

describe('formatClock', () => {
	it('renders minutes and seconds under an hour', () => {
		expect(formatClock(0)).toBe('0:00');
		expect(formatClock(65)).toBe('1:05');
		expect(formatClock(3599)).toBe('59:59');
	});

	it('adds an hours figure past an hour', () => {
		expect(formatClock(3600)).toBe('1:00:00');
		expect(formatClock(5025)).toBe('1:23:45');
	});

	it('carries hours past twenty-four without wrapping', () => {
		// The forgotten weekend: 65 hours, 12 minutes, 3 seconds.
		expect(formatClock(65 * 3600 + 12 * 60 + 3)).toBe('65:12:03');
	});

	it('clamps negatives to zero', () => {
		expect(formatClock(-5)).toBe('0:00');
	});
});

describe('anchoredElapsedSeconds', () => {
	it('adds the local seconds since the anchor', () => {
		expect(anchoredElapsedSeconds(95, 1000, 4200)).toBe(98);
	});

	it('never counts backwards on a stale-looking anchor', () => {
		expect(anchoredElapsedSeconds(95, 4200, 1000)).toBe(95);
	});
});
