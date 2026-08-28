<!--
  Dispatches a `ViewData` payload to the matching per-kind component.

  Every variant of the discriminated union has a branch below, and
  `unrenderedKind` makes that a compile-time requirement rather than a
  convention: in the final `{:else}` branch `data` is narrowed to `never`
  only while the chain is exhaustive, so a variant without a branch fails
  `npm run check` and is named in the error. The placeholder it guards is
  for a stale bundle talking to a newer server.
-->
<script lang="ts">
	import type { ViewData } from '$lib/api/generated/ViewData';
	import BoardView from './board/BoardView.svelte';
	import TableView from './table/TableView.svelte';
	import TreeView from './tree/TreeView.svelte';
	import GraphView from './graph/GraphView.svelte';
	import GanttView from './gantt/GanttView.svelte';
	import GanttByDepthView from './gantt/GanttByDepthView.svelte';
	import GanttByInitiativeView from './gantt/GanttByInitiativeView.svelte';
	import MetricView from './metric/MetricView.svelte';
	import BarChartView from './bar_chart/BarChartView.svelte';
	import LineChartView from './line_chart/LineChartView.svelte';
	import WorkloadView from './workload/WorkloadView.svelte';
	import HeatmapView from './heatmap/HeatmapView.svelte';
	import TreemapView from './treemap/TreemapView.svelte';

	interface Props {
		data: ViewData;
	}

	let { data }: Props = $props();

	// Takes `never`, so it only type-checks while every view kind above has
	// its own branch, as the comment at the top of this file explains. The
	// cast reads a `type` off a value the type system says cannot exist,
	// which is exactly the stale-bundle case the placeholder explains.
	function unrenderedKind(data: never): string {
		return (data as { type: string }).type;
	}
</script>

{#if data.type === 'board'}
	<BoardView {data} />
{:else if data.type === 'table'}
	<TableView {data} />
{:else if data.type === 'tree'}
	<TreeView {data} />
{:else if data.type === 'graph'}
	<GraphView {data} />
{:else if data.type === 'gantt'}
	<GanttView {data} />
{:else if data.type === 'gantt_by_depth'}
	<GanttByDepthView {data} />
{:else if data.type === 'gantt_by_initiative'}
	<GanttByInitiativeView {data} />
{:else if data.type === 'metric'}
	<MetricView {data} />
{:else if data.type === 'bar_chart'}
	<BarChartView {data} />
{:else if data.type === 'line_chart'}
	<LineChartView {data} />
{:else if data.type === 'workload'}
	<WorkloadView {data} />
{:else if data.type === 'heatmap'}
	<HeatmapView {data} />
{:else if data.type === 'treemap'}
	<TreemapView {data} />
{:else}
	<div class="placeholder">
		<p>
			View kind <code>{unrenderedKind(data)}</code> is not yet rendered.
		</p>
		<p class="hint">Regenerate types after a backend addition and add the matching branch above.</p>
	</div>
{/if}

<style>
	.placeholder {
		padding: var(--space-6);
		border: 1px dashed var(--color-border);
		border-radius: var(--radius-md);
		color: var(--color-fg-muted);
		text-align: center;
	}

	.placeholder p {
		margin: 0 0 var(--space-2);
	}

	.placeholder p:last-child {
		margin-bottom: 0;
	}

	.hint {
		font-size: var(--text-sm);
	}
</style>
