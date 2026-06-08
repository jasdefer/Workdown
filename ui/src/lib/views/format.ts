// Shared value formatters for chart-family views.
//
// AggregateValue / AxisValue / SizeValue all share the same tagged
// shape on the wire (`{type, value}`); these helpers cover the three
// variant types the Rust enums can produce. Lifted out of MetricView
// when BarChartView became the second consumer.

import type { AggregateValue } from '$lib/api/generated/AggregateValue';

/**
 * Format an aggregate value (or null) for display in a KPI tile,
 * tooltip, or axis tick label. Null renders as the em-dash "no data"
 * placeholder used by the markdown renderer in
 * crates/cli/src/render/metric.rs.
 */
export function formatAggregateValue(value: AggregateValue | null): string {
	if (value === null) return '—';
	if (value.type === 'duration') return formatDurationSeconds(value.value);
	if (value.type === 'date') return value.value;
	return formatNumber(value.value);
}

/**
 * Drop the integer fraction for whole numbers within the safe integer
 * range; pass non-integers through with native toString. Matches
 * `format_number` in crates/cli/src/render/markdown.rs.
 */
export function formatNumber(n: number): string {
	if (Number.isFinite(n) && Number.isInteger(n) && Math.abs(n) < 1e15) {
		return n.toFixed(0);
	}
	return n.toString();
}

/**
 * Render a canonical-seconds duration as suffix shorthand. Defaults to
 * compact form (at most two most-significant units) for UI display —
 * `2d 13h` rather than `2d 13h 6min 40s`. The Rust-side
 * `format_duration_seconds` in crates/core/src/model/duration.rs emits
 * full precision for the markdown renderers, intentional divergence.
 *
 * Pass `maxUnits = Infinity` for full precision when needed.
 */
export function formatDurationSeconds(seconds: number, maxUnits = 2): string {
	if (seconds === 0) return '0s';
	const sign = seconds < 0 ? '-' : '';
	let remaining = Math.abs(seconds);
	const units: [number, string][] = [
		[604800, 'w'],
		[86400, 'd'],
		[3600, 'h'],
		[60, 'min'],
		[1, 's']
	];
	const parts: string[] = [];
	for (const [size, suffix] of units) {
		const count = Math.floor(remaining / size);
		if (count > 0) {
			parts.push(count.toString() + suffix);
			remaining -= count * size;
			if (parts.length >= maxUnits) break;
		}
	}
	return sign + parts.join(' ');
}

/**
 * Format a Date as a local `YYYY-MM-DD` string, for date-axis tick labels
 * and the gantt "today" marker. Uses local calendar parts — this is a
 * verbatim lift of the formatter the chart views each carried inline; the
 * local-vs-UTC choice is preserved deliberately, not revisited here.
 */
export function formatIsoDate(date: Date): string {
	const pad = (v: number): string => v.toString().padStart(2, '0');
	return `${date.getFullYear().toString()}-${pad(date.getMonth() + 1)}-${pad(date.getDate())}`;
}

/**
 * Pick a duration unit (weeks / days / hours / minutes / seconds)
 * appropriate for an axis whose maximum value is `maxSeconds`. The
 * caller divides values by `seconds` to produce small whole numbers
 * for the axis ticks, and appends `label` to the axis title.
 *
 * Mirrors `pick_duration_unit` in crates/cli/src/render/svg_chart.rs,
 * which serves the same role for the server-side SVG renderers.
 */
export function pickDurationUnit(maxSeconds: number): { seconds: number; label: string } {
	const abs = Math.abs(maxSeconds);
	if (abs >= 604800) return { seconds: 604800, label: 'weeks' };
	if (abs >= 86400) return { seconds: 86400, label: 'days' };
	if (abs >= 3600) return { seconds: 3600, label: 'hours' };
	if (abs >= 60) return { seconds: 60, label: 'minutes' };
	return { seconds: 1, label: 'seconds' };
}

/**
 * Format a scalar magnitude that is either a duration (canonical
 * seconds, rendered as suffix shorthand like `2d 13h`) or a plain
 * number. The duration-vs-number branch shared by workload totals and
 * treemap sizes — the untagged-value counterpart of
 * [`formatAggregateValue`].
 */
export function formatScalar(value: number, isDuration: boolean): string {
	return isDuration ? formatDurationSeconds(value) : formatNumber(value);
}

/**
 * Format a count with its noun, pluralizing for counts other than one:
 * `pluralize(1, 'item')` → "1 item", `pluralize(5, 'bar')` → "5 bars".
 * The naive "+s" rule covers every view-chrome noun (item, bar, cell,
 * point, working day); pass an explicit `plural` for an irregular noun.
 */
export function pluralize(count: number, singular: string, plural = `${singular}s`): string {
	return `${count.toString()} ${count === 1 ? singular : plural}`;
}
