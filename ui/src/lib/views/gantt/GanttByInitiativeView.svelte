<!--
  Gantt by initiative. Partitions the chart's bars into bands by the
  root of the view's configured link chain (e.g. `parent` → top-level
  initiative). All initiatives share one axis (they're sections of a
  single <GanttChart>), so two initiatives' timelines line up visually.
  Band labels are the root card's title with an id fallback, matching
  the Markdown renderer's per-chart heading.
-->
<script lang="ts">
	import type { GanttByInitiativeData } from '$lib/api/generated/GanttByInitiativeData';
	import { prettifyId } from '$lib/views/prettify';
	import GanttChart, { type GanttSection } from './GanttChart.svelte';
	import UnplacedFooter from '$lib/views/UnplacedFooter.svelte';

	interface Props {
		data: GanttByInitiativeData;
	}

	let { data }: Props = $props();

	const totalBars = $derived(
		data.initiatives.reduce((sum, initiative) => sum + initiative.bars.length, 0)
	);

	const sections = $derived.by<GanttSection[]>(() =>
		data.initiatives.map((initiative) => ({
			label: initiative.root.title ?? prettifyId(initiative.root.id),
			bars: initiative.bars
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
