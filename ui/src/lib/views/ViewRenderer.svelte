<!--
  Dispatches a `ViewData` payload to the matching per-kind component.
  In slice 2 only the board has a real component; other variants fall
  through to a placeholder until their UI lands in
  `remaining-read-views`.
-->
<script lang="ts">
	import type { ViewData } from '$lib/api/generated/ViewData';
	import BoardView from './board/BoardView.svelte';
	import TableView from './table/TableView.svelte';
	import TreeView from './tree/TreeView.svelte';
	import GraphView from './graph/GraphView.svelte';
	import GanttView from './gantt/GanttView.svelte';

	interface Props {
		data: ViewData;
	}

	let { data }: Props = $props();
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
{:else}
	<div class="placeholder">
		<p>View kind <code>{data.type}</code> is not yet rendered.</p>
		<p class="hint">
			This view will gain its UI in a later slice (<code>remaining-read-views</code>).
		</p>
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

	.placeholder code {
		font-family: var(--font-mono);
		background-color: var(--color-surface);
		padding: 0.1em 0.3em;
		border-radius: var(--radius-md);
	}

	.hint {
		font-size: var(--text-sm);
	}
</style>
