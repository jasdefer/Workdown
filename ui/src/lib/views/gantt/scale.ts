// Time-axis math for the gantt view. Pure and DOM-free so it can be unit
// tested in isolation — every layout decision (range, granularity, tick
// generation, date→x mapping) lives here, and the Svelte components are
// thin presentation over the returned structs.
//
// Dates arrive from the wire as inclusive `"YYYY-MM-DD"` strings. We work
// in *epoch-day integers* (whole days since 1970-01-01, UTC) so there are
// no timezone or daylight-saving pitfalls: a date is a count of days, and
// every offset is integer subtraction.

const MS_PER_DAY = 86_400_000;
const MONTH_NAMES = [
	'Jan',
	'Feb',
	'Mar',
	'Apr',
	'May',
	'Jun',
	'Jul',
	'Aug',
	'Sep',
	'Oct',
	'Nov',
	'Dec'
];

/** Tick granularity, chosen by total span. */
export type Granularity = 'day' | 'week' | 'month';

/** Pixels per day at each granularity. Density shrinks as the span grows
 *  so a multi-year chart stays scrollable rather than tens of thousands of
 *  pixels wide. Tuned by eye; the single knob that sets overall zoom. */
export const PX_PER_DAY: Record<Granularity, number> = {
	day: 28,
	week: 10,
	month: 4
};

/** Minimum rendered bar width, so a 1-day bar stays visible. */
export const MIN_BAR_WIDTH = 6;

/** Inclusive epoch-day range `[startDay, endDay]`. */
export interface DateRange {
	startDay: number;
	endDay: number;
}

/** A minor tick: a labelled vertical line on the time axis. */
export interface Tick {
	x: number;
	label: string;
}

/** A major period band (months under day/week ticks; years under month
 *  ticks). `x`/`width` are clamped to the visible range. */
export interface Period {
	x: number;
	width: number;
	label: string;
}

export interface Axis {
	granularity: Granularity;
	pxPerDay: number;
	chartWidth: number;
	ticks: Tick[];
	periods: Period[];
}

function monthName(month1: number): string {
	return MONTH_NAMES[month1 - 1] ?? '';
}

/** Parse an inclusive `"YYYY-MM-DD"` date into an epoch-day integer. */
export function parseDay(iso: string): number {
	const parts = iso.split('-');
	const year = Number(parts[0]);
	const month = Number(parts[1]);
	const day = Number(parts[2]);
	return Math.round(Date.UTC(year, month - 1, day) / MS_PER_DAY);
}

function partsOf(day: number): { year: number; month: number; day: number } {
	const date = new Date(day * MS_PER_DAY);
	return {
		year: date.getUTCFullYear(),
		month: date.getUTCMonth() + 1,
		day: date.getUTCDate()
	};
}

function dayFromYMD(year: number, month1: number, day: number): number {
	return Math.round(Date.UTC(year, month1 - 1, day) / MS_PER_DAY);
}

function monthStart(day: number): number {
	const parts = partsOf(day);
	return dayFromYMD(parts.year, parts.month, 1);
}

function nextMonthStart(day: number): number {
	const parts = partsOf(day);
	return parts.month === 12
		? dayFromYMD(parts.year + 1, 1, 1)
		: dayFromYMD(parts.year, parts.month + 1, 1);
}

function yearStart(day: number): number {
	return dayFromYMD(partsOf(day).year, 1, 1);
}

function nextYearStart(day: number): number {
	return dayFromYMD(partsOf(day).year + 1, 1, 1);
}

/** Monday-based week start for an epoch day. */
function weekStart(day: number): number {
	const weekday = new Date(day * MS_PER_DAY).getUTCDay(); // 0 Sun .. 6 Sat
	const sinceMonday = (weekday + 6) % 7;
	return day - sinceMonday;
}

/** Inclusive day count of a range. */
export function spanDays(range: DateRange): number {
	return range.endDay - range.startDay + 1;
}

/** Pick tick granularity from a span: days up to ~6 weeks, weeks up to
 *  ~1 year, months beyond. */
export function chooseGranularity(span: number): Granularity {
	if (span <= 42) return 'day';
	if (span <= 366) return 'week';
	return 'month';
}

/** Raw inclusive bounds across bars, or null when there are no bars. */
export function boundsOf(bars: { start: string; end: string }[]): DateRange | null {
	if (bars.length === 0) return null;
	let startDay = Infinity;
	let endDay = -Infinity;
	for (const bar of bars) {
		startDay = Math.min(startDay, parseDay(bar.start));
		endDay = Math.max(endDay, parseDay(bar.end));
	}
	return { startDay, endDay };
}

/** Snap bounds outward to clean boundaries so ticks line up: a day or two
 *  of padding for day granularity, whole weeks for week, whole months for
 *  month. */
export function snapRange(bounds: DateRange, granularity: Granularity): DateRange {
	switch (granularity) {
		case 'day':
			return { startDay: bounds.startDay - 1, endDay: bounds.endDay + 1 };
		case 'week':
			return { startDay: weekStart(bounds.startDay), endDay: weekStart(bounds.endDay) + 6 };
		case 'month':
			return { startDay: monthStart(bounds.startDay), endDay: nextMonthStart(bounds.endDay) - 1 };
	}
}

/** Compute the snapped range + granularity for a set of bars, or null when
 *  empty. Granularity is chosen from the raw span (before snapping) so a
 *  range sitting just under a threshold doesn't flip category from the
 *  snap padding alone. */
export function computeRange(
	bars: { start: string; end: string }[]
): { range: DateRange; granularity: Granularity } | null {
	const bounds = boundsOf(bars);
	if (bounds === null) return null;
	const granularity = chooseGranularity(spanDays(bounds));
	return { range: snapRange(bounds, granularity), granularity };
}

/** Build the axis (ticks + period bands + total width) for a range. */
export function buildAxis(range: DateRange, granularity: Granularity): Axis {
	const pxPerDay = PX_PER_DAY[granularity];
	const chartWidth = spanDays(range) * pxPerDay;
	const x = (day: number): number => (day - range.startDay) * pxPerDay;

	const ticks: Tick[] = [];
	const periods: Period[] = [];

	const pushMonthPeriods = (): void => {
		let cursor = monthStart(range.startDay);
		while (cursor <= range.endDay) {
			const start = Math.max(cursor, range.startDay);
			const end = Math.min(nextMonthStart(cursor) - 1, range.endDay);
			const parts = partsOf(cursor);
			periods.push({
				x: x(start),
				width: (end - start + 1) * pxPerDay,
				label: `${monthName(parts.month)} ${parts.year.toString()}`
			});
			cursor = nextMonthStart(cursor);
		}
	};

	if (granularity === 'day') {
		for (let day = range.startDay; day <= range.endDay; day++) {
			ticks.push({ x: x(day), label: partsOf(day).day.toString() });
		}
		pushMonthPeriods();
	} else if (granularity === 'week') {
		for (let day = weekStart(range.startDay); day <= range.endDay; day += 7) {
			const start = Math.max(day, range.startDay);
			const parts = partsOf(start);
			ticks.push({ x: x(start), label: `${parts.day.toString()} ${monthName(parts.month)}` });
		}
		pushMonthPeriods();
	} else {
		let cursor = monthStart(range.startDay);
		while (cursor <= range.endDay) {
			ticks.push({ x: x(cursor), label: monthName(partsOf(cursor).month) });
			cursor = nextMonthStart(cursor);
		}
		let year = yearStart(range.startDay);
		while (year <= range.endDay) {
			const start = Math.max(year, range.startDay);
			const end = Math.min(nextYearStart(year) - 1, range.endDay);
			periods.push({
				x: x(start),
				width: (end - start + 1) * pxPerDay,
				label: partsOf(year).year.toString()
			});
			year = nextYearStart(year);
		}
	}

	return { granularity, pxPerDay, chartWidth, ticks, periods };
}

/** Left offset + width (px) for a bar's inclusive `[start, end]` window.
 *  Inclusive end means a same-day bar is one day wide; sub-MIN_BAR_WIDTH
 *  windows are floored so 1-day bars stay visible. */
export function barGeometry(
	startIso: string,
	endIso: string,
	range: DateRange,
	pxPerDay: number
): { left: number; width: number } {
	const startDay = parseDay(startIso);
	const endDay = parseDay(endIso);
	const left = (startDay - range.startDay) * pxPerDay;
	const width = Math.max(MIN_BAR_WIDTH, (endDay - startDay + 1) * pxPerDay);
	return { left, width };
}

/** X offset of a date within the range, or null when it falls outside —
 *  used for the "today" marker. */
export function offsetForDate(iso: string, range: DateRange, pxPerDay: number): number | null {
	const day = parseDay(iso);
	if (day < range.startDay || day > range.endDay) return null;
	return (day - range.startDay) * pxPerDay;
}
