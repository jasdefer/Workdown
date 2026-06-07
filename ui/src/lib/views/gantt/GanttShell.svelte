<!--
  Shared chrome for the three gantt wrappers (basic / by-depth /
  by-initiative): the empty hint (only shown when nothing was dropped
  either — otherwise the unplaced footer already explains the empty
  chart), the chart itself, the unplaced footer, and the item count.
  The wrappers differ only in how they map their wire data into
  `sections`; this shell owns everything else.
-->
<script lang="ts">
	import type { UnplacedCard } from '$lib/api/generated/UnplacedCard';
	import EmptyHint from '$lib/views/EmptyHint.svelte';
	import RowCount from '$lib/views/RowCount.svelte';
	import UnplacedFooter from '$lib/views/UnplacedFooter.svelte';
	import GanttChart, { type GanttSection } from './GanttChart.svelte';

	interface Props {
		sections: GanttSection[];
		count: number;
		unplaced: UnplacedCard[];
	}

	let { sections, count, unplaced }: Props = $props();
</script>

{#if count === 0}
	{#if unplaced.length === 0}
		<EmptyHint />
	{/if}
{:else}
	<GanttChart {sections} />
{/if}

<UnplacedFooter {unplaced} />

<RowCount {count} />
