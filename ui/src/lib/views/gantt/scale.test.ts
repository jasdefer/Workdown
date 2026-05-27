import { describe, expect, it } from 'vitest';

import {
	MIN_BAR_WIDTH,
	PX_PER_DAY,
	barGeometry,
	boundsOf,
	buildAxis,
	chooseGranularity,
	computeRange,
	offsetForDate,
	parseDay,
	snapRange,
	spanDays
} from './scale';

describe('parseDay', () => {
	it('counts whole days between dates', () => {
		expect(parseDay('2026-01-05') - parseDay('2026-01-01')).toBe(4);
	});

	it('spans month boundaries correctly', () => {
		// January has 31 days, so Feb 1 is 31 days after Jan 1.
		expect(parseDay('2026-02-01') - parseDay('2026-01-01')).toBe(31);
	});

	it('spans leap years correctly', () => {
		// 2028 is a leap year (366 days).
		expect(parseDay('2029-01-01') - parseDay('2028-01-01')).toBe(366);
	});
});

describe('chooseGranularity', () => {
	it('uses days up to six weeks', () => {
		expect(chooseGranularity(1)).toBe('day');
		expect(chooseGranularity(42)).toBe('day');
	});

	it('uses weeks up to a year', () => {
		expect(chooseGranularity(43)).toBe('week');
		expect(chooseGranularity(366)).toBe('week');
	});

	it('uses months beyond a year', () => {
		expect(chooseGranularity(367)).toBe('month');
		expect(chooseGranularity(1000)).toBe('month');
	});
});

describe('boundsOf', () => {
	it('returns null for no bars', () => {
		expect(boundsOf([])).toBeNull();
	});

	it('takes the min start and max end across bars', () => {
		const bounds = boundsOf([
			{ start: '2026-01-10', end: '2026-01-12' },
			{ start: '2026-01-05', end: '2026-01-20' },
			{ start: '2026-01-08', end: '2026-01-09' }
		]);
		expect(bounds).toEqual({ startDay: parseDay('2026-01-05'), endDay: parseDay('2026-01-20') });
	});
});

describe('snapRange', () => {
	it('pads a day or so for day granularity', () => {
		const bounds = { startDay: parseDay('2026-01-10'), endDay: parseDay('2026-01-12') };
		const snapped = snapRange(bounds, 'day');
		expect(snapped.startDay).toBe(parseDay('2026-01-09'));
		expect(snapped.endDay).toBe(parseDay('2026-01-13'));
	});

	it('snaps to whole weeks (Monday..Sunday) for week granularity', () => {
		// 2026-01-10 is a Saturday; the week starts Monday 2026-01-05.
		// 2026-03-01 is a Sunday; its week ends that same Sunday.
		const bounds = { startDay: parseDay('2026-01-10'), endDay: parseDay('2026-03-01') };
		const snapped = snapRange(bounds, 'week');
		expect(snapped.startDay).toBe(parseDay('2026-01-05'));
		expect(snapped.endDay).toBe(parseDay('2026-03-01'));
	});

	it('snaps to whole months for month granularity', () => {
		const bounds = { startDay: parseDay('2026-02-10'), endDay: parseDay('2026-05-20') };
		const snapped = snapRange(bounds, 'month');
		expect(snapped.startDay).toBe(parseDay('2026-02-01'));
		expect(snapped.endDay).toBe(parseDay('2026-05-31'));
	});
});

describe('computeRange', () => {
	it('returns null for no bars', () => {
		expect(computeRange([])).toBeNull();
	});

	it('chooses granularity from the raw span, then snaps', () => {
		const result = computeRange([{ start: '2026-01-10', end: '2026-01-15' }]);
		expect(result?.granularity).toBe('day');
		expect(result?.range.startDay).toBe(parseDay('2026-01-09'));
	});
});

describe('buildAxis', () => {
	it('chart width is span × density', () => {
		const range = { startDay: parseDay('2026-01-01'), endDay: parseDay('2026-01-10') };
		const axis = buildAxis(range, 'day');
		expect(spanDays(range)).toBe(10);
		expect(axis.chartWidth).toBe(10 * PX_PER_DAY.day);
	});

	it('emits one day tick per day and a month period band', () => {
		const range = { startDay: parseDay('2026-01-01'), endDay: parseDay('2026-01-05') };
		const axis = buildAxis(range, 'day');
		expect(axis.ticks).toHaveLength(5);
		expect(axis.ticks[0]).toEqual({ x: 0, label: '1' });
		expect(axis.periods).toHaveLength(1);
		expect(axis.periods[0]?.label).toBe('Jan 2026');
	});

	it('emits year period bands at month granularity', () => {
		const range = { startDay: parseDay('2026-01-01'), endDay: parseDay('2027-12-31') };
		const axis = buildAxis(range, 'month');
		expect(axis.ticks).toHaveLength(24); // 24 months
		expect(axis.periods.map((p) => p.label)).toEqual(['2026', '2027']);
	});
});

describe('barGeometry', () => {
	it('places the bar by its start offset', () => {
		const range = { startDay: parseDay('2026-01-01'), endDay: parseDay('2026-01-31') };
		const geom = barGeometry('2026-01-05', '2026-01-09', range, PX_PER_DAY.day);
		expect(geom.left).toBe(4 * PX_PER_DAY.day);
		// Inclusive: Jan 5..9 is 5 days wide.
		expect(geom.width).toBe(5 * PX_PER_DAY.day);
	});

	it('floors a same-day bar to the minimum width', () => {
		const range = { startDay: parseDay('2026-01-01'), endDay: parseDay('2026-01-31') };
		// One day × 4px/day (month density) = 4px, below the floor.
		const geom = barGeometry('2026-01-05', '2026-01-05', range, PX_PER_DAY.month);
		expect(geom.width).toBe(MIN_BAR_WIDTH);
	});
});

describe('offsetForDate', () => {
	const range = { startDay: parseDay('2026-01-01'), endDay: parseDay('2026-01-31') };

	it('returns the offset for a date inside the range', () => {
		expect(offsetForDate('2026-01-11', range, PX_PER_DAY.day)).toBe(10 * PX_PER_DAY.day);
	});

	it('returns null for a date outside the range', () => {
		expect(offsetForDate('2025-12-31', range, PX_PER_DAY.day)).toBeNull();
		expect(offsetForDate('2026-02-01', range, PX_PER_DAY.day)).toBeNull();
	});
});
