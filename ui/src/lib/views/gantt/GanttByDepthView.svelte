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
	import UnplacedFooter from '$lib/views/UnplacedFooter.svelte';
	import EmptyHint from '$lib/views/EmptyHint.svelte';
	import RowCount from '$lib/views/RowCount.svelte';

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
</script>

{#if totalBars === 0}
	{#if data.unplaced.length === 0}
		<EmptyHint />
	{/if}
{:else}
	<GanttChart {sections} />
{/if}

<UnplacedFooter unplaced={data.unplaced} />

<RowCount count={totalBars} />
