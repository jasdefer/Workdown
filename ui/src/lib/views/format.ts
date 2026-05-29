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
 * Render a canonical-seconds duration as suffix shorthand
 * (`1w 2d 3h 4min 5s`). Mirrors `format_duration_seconds` in
 * crates/core/src/model/duration.rs.
 */
export function formatDurationSeconds(seconds: number): string {
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
		}
	}
	return sign + parts.join(' ');
}
