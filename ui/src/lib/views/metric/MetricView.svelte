<!--
  Metric view. Each row in MetricData.rows renders as a KPI tile —
  hero value at the top, smaller label below, optional "N items
  dropped" footer when the row has unplaced items (filter-matched but
  missing the value field needed for aggregation).

  Value formatting is shared with the chart-family views via
  `$lib/views/format` — mirrors the markdown renderer's rules in
  crates/cli/src/render/metric.rs.
-->
<script lang="ts">
	import type { MetricData } from '$lib/api/generated/MetricData';
	import { formatAggregateValue, pluralize } from '$lib/views/format';
	import EmptyHint from '$lib/views/EmptyHint.svelte';

	interface Props {
		data: MetricData;
	}

	let { data }: Props = $props();
</script>

{#if data.rows.length === 0}
	<EmptyHint message="No metrics to display." />
{:else}
	<div class="metric-grid" role="region" aria-label="Metric view">
		{#each data.rows as row, index (index)}
			<article class="tile">
				<span class="value" class:none={row.value === null}>{formatAggregateValue(row.value)}</span>
				<span class="label">{row.label}</span>
				{#if row.unplaced.length > 0}
					<span class="dropped">
						{pluralize(row.unplaced.length, 'item')} dropped
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
</style>
