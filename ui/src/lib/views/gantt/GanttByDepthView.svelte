<!--
  Gantt by depth. Partitions the chart's bars into bands by their depth
  in the view's configured link chain — level 0 = roots, level 1 =
  their direct children, etc. All levels share one axis (they're sections
  of a single <GanttChart>) so timing reads across levels at a glance.
  Band labels are `Level <n>`, matching the Markdown renderer's `## Level
  <n>` heading.
-->
<script lang="ts">
	import type { GanttByDepthData } from '$lib/api/generated/GanttByDepthData';
	import GanttChart, { type GanttSection } from './GanttChart.svelte';
	import UnplacedFooter from './UnplacedFooter.svelte';

	interface Props {
		data: GanttByDepthData;
	}

	let { data }: Props = $props();

	const totalBars = $derived(data.levels.reduce((sum, level) => sum + level.bars.length, 0));

	const sections = $derived.by<GanttSection[]>(() =>
		data.levels.map((level) => ({
			label: `Level ${level.depth.toString()}`,
			bars: level.bars
		}))
	);

	const countLabel = $derived(
		totalBars === 1 ? 'Showing 1 item' : `Showing ${totalBars.toString()} items`
	);
</script>

{#if totalBars === 0}
	{#if data.unplaced.length === 0}
		<p class="empty-hint">No items to display.</p>
	{/if}
{:else}
	<GanttChart {sections} />
{/if}

<UnplacedFooter unplaced={data.unplaced} />

{#if totalBars > 0}
	<p class="row-count">{countLabel}</p>
{/if}

<style>
	.empty-hint {
		color: var(--color-fg-muted);
		font-size: var(--text-sm);
		margin: 0 0 var(--space-3);
	}

	.row-count {
		margin: var(--space-2) 0 0;
		font-size: var(--text-sm);
		color: var(--color-fg-muted);
	}
</style>
