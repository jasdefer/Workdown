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
	import { cardLabel } from '$lib/views/prettify';
	import type { GanttSection } from './GanttChart.svelte';
	import GanttShell from './GanttShell.svelte';

	interface Props {
		data: GanttByInitiativeData;
	}

	let { data }: Props = $props();

	const totalBars = $derived(
		data.initiatives.reduce((sum, initiative) => sum + initiative.bars.length, 0)
	);

	const sections = $derived.by<GanttSection[]>(() =>
		data.initiatives.map((initiative) => ({
			label: cardLabel(initiative.root),
			bars: initiative.bars
		}))
	);
</script>

<GanttShell {sections} count={totalBars} unplaced={data.unplaced} />
