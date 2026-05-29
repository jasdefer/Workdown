<!--
  Metric view. Each row in MetricData.rows renders as a KPI tile —
  hero value at the top, smaller label below, optional "N items
  dropped" footer when the row has unplaced items (filter-matched but
  missing the value field needed for aggregation).

  Value formatting mirrors the markdown renderer's rules in
  crates/cli/src/render/metric.rs:
    null                          → "—"           no data (avg/min/max over empty set)
    {type: "number", value}       → drop integer fraction         (12, 3.5)
    {type: "date",   value}       → pass through                  (already ISO YYYY-MM-DD)
    {type: "duration", value}     → seconds → suffix shorthand    (1d 1h, 30min)

  AggregateValue is tagged on the wire so the frontend can recover the
  variant (JSON has no bigint; an untagged i64 would arrive as a JS
  number indistinguishable from a Number variant). Formatter kept
  inline rather than in lib/views/ until the bar slice surfaces a
  second consumer.
-->
<script lang="ts">
	import type { MetricData } from '$lib/api/generated/MetricData';
	import type { AggregateValue } from '$lib/api/generated/AggregateValue';

	interface Props {
		data: MetricData;
	}

	let { data }: Props = $props();

	function formatValue(value: AggregateValue | null): string {
		if (value === null) return '—';
		if (value.type === 'duration') return formatDurationSeconds(value.value);
		if (value.type === 'date') return value.value;
		return formatNumber(value.value);
	}

	function formatNumber(n: number): string {
		if (Number.isFinite(n) && Number.isInteger(n) && Math.abs(n) < 1e15) {
			return n.toFixed(0);
		}
		return n.toString();
	}

	function formatDurationSeconds(seconds: number): string {
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
</script>

{#if data.rows.length === 0}
	<p class="empty-hint">No metrics to display.</p>
{:else}
	<div class="metric-grid" role="region" aria-label="Metric view">
		{#each data.rows as row, index (index)}
			<article class="tile">
				<span class="value" class:none={row.value === null}>{formatValue(row.value)}</span>
				<span class="label">{row.label}</span>
				{#if row.unplaced.length > 0}
					<span class="dropped">
						{row.unplaced.length}
						{row.unplaced.length === 1 ? 'item' : 'items'} dropped
					</span>
				{/if}
			</article>
		{/each}
	</div>
{/if}

<style>
	.metric-grid {
		display: flex;
		flex-wrap: wrap;
		gap: var(--space-4);
		align-content: flex-start;
	}

	.tile {
		background-color: var(--color-surface);
		border-radius: var(--radius-md);
		padding: var(--space-5) var(--space-6);
		display: flex;
		flex-direction: column;
		gap: var(--space-1);
		min-width: 12rem;
		flex: 0 1 auto;
		box-shadow: var(--shadow-sm);
	}

	.value {
		font-size: 3rem;
		font-weight: 600;
		line-height: 1.05;
		font-variant-numeric: tabular-nums;
		color: var(--color-fg);
		overflow-wrap: anywhere;
	}

	.value.none {
		color: var(--color-fg-muted);
	}

	.label {
		font-size: var(--text-sm);
		color: var(--color-fg-muted);
	}

	.dropped {
		font-size: var(--text-sm);
		color: var(--color-fg-muted);
		margin-top: var(--space-2);
	}

	.empty-hint {
		color: var(--color-fg-muted);
		font-size: var(--text-sm);
		margin: 0 0 var(--space-3);
	}
</style>
