<!--
  What the tints of a view mean: one quiet row above the view, a
  caption ("Color by Status") and a swatch per color on screen with the
  number of items wearing it. For a color the schema can only trace,
  not explain (a pulled one), the caption stands alone.

  The server decides what there is to say (`ColorLegend`, absent when
  the view is untinted or the color field has no recipe);
  `colorLegend.ts` words it. A long condition is cut with an ellipsis
  and readable in full on hover.
-->
<script lang="ts">
	import type { ColorLegend } from '$lib/api/generated/ColorLegend';
	import { legendModel } from './colorLegend';

	interface Props {
		legend: ColorLegend;
	}

	let { legend }: Props = $props();

	const model = $derived(legendModel(legend));
</script>

<div class="color-legend">
	<span class="caption">{model.caption}</span>
	{#if model.entries.length > 0}
		<ul class="entries" aria-label="Color legend">
			{#each model.entries as entry, index (index)}
				<li class="entry" title={entry.label}>
					<span class="swatch" style:background-color={entry.color} aria-hidden="true"></span>
					<span class="label">{entry.label}</span>
					<span class="count">{entry.count}</span>
				</li>
			{/each}
		</ul>
	{/if}
</div>

<style>
	.color-legend {
		display: flex;
		flex-wrap: wrap;
		align-items: baseline;
		gap: var(--space-1) var(--space-4);
		font-size: var(--text-sm);
		color: var(--color-fg-muted);
	}

	.caption {
		flex-shrink: 0;
	}

	.entries {
		display: flex;
		flex-wrap: wrap;
		gap: var(--space-1) var(--space-4);
		margin: 0;
		padding: 0;
		list-style: none;
		min-width: 0;
	}

	.entry {
		display: inline-flex;
		align-items: baseline;
		gap: var(--space-2);
		min-width: 0;
		max-width: 24rem;
	}

	.swatch {
		flex-shrink: 0;
		align-self: center;
		width: 0.75rem;
		height: 0.75rem;
		border-radius: var(--radius-full);
		border: 1px solid var(--color-border);
	}

	.label {
		color: var(--color-fg);
		overflow: hidden;
		text-overflow: ellipsis;
		white-space: nowrap;
	}

	.count {
		flex-shrink: 0;
		font-variant-numeric: tabular-nums;
	}
</style>
