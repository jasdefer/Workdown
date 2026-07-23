<!--
  Hierarchical tree view. Outline grid: one row per visible node with
  the hierarchy in the first column (sticky on horizontal scroll) and
  user-configured fields in subsequent columns. Identical column
  rendering to TableView — Cell, Chip, prettifyId are all shared.

  Expansion state is session-local component state (a reactive Set of
  expanded node ids). All nodes start collapsed: only roots are visible
  on first paint. Persistence (URL / localStorage / views.yaml) is
  deferred to the shared view-display-config decision.

  Column widths default to auto (1fr for the hierarchy column,
  max-content for data columns). Once the user drags a header divider,
  that column switches to a fixed pixel width via the `columnWidths`
  reactive map, keyed by grid column index (0 = hierarchy, 1..N = data
  columns). Persistence joins the same view-display-config bucket as
  expansion state.

  Each node renders as N+1 direct grid children of `.tree`, wrapped in
  a `display: contents` row container so ARIA `role="row"` works
  without disturbing grid layout.

  When `columns` is empty the view degenerates to a single-column
  outline — no header row, no per-row data cells, no resize handles.
-->
<script lang="ts">
	import type { TreeData } from '$lib/api/generated/TreeData';
	import type { TreeNode } from '$lib/api/generated/TreeNode';
	import { SvelteMap, SvelteSet } from 'svelte/reactivity';
	import { prettifyId } from '$lib/views/prettify';
	import ColumnResizeHandle from '$lib/views/ColumnResizeHandle.svelte';
	import EmptyHint from '$lib/views/EmptyHint.svelte';
	import RowCount from '$lib/views/RowCount.svelte';
	import TreeNodeRow from './TreeNode.svelte';

	interface Props {
		data: TreeData;
	}

	let { data }: Props = $props();

	let expandedIds = $state(new SvelteSet<string>());
	let columnWidths = $state(new SvelteMap<number, number>());

	function toggle(id: string) {
		if (expandedIds.has(id)) {
			expandedIds.delete(id);
		} else {
			expandedIds.add(id);
		}
	}

	function nodeCount(nodes: TreeNode[]): number {
		let total = 0;
		for (const node of nodes) {
			total += 1 + nodeCount(node.children);
		}
		return total;
	}

	const totalNodes = $derived(nodeCount(data.roots));

	function trackWidth(index: number, fallback: string): string {
		const set = columnWidths.get(index);
		return set === undefined ? fallback : `${set.toString()}px`;
	}

	const gridTemplate = $derived.by(() => {
		const titleTrack = trackWidth(0, 'minmax(20rem, 1fr)');
		if (data.columns.length === 0) return titleTrack;
		const dataTracks = data.columns.map((_, i) => trackWidth(i + 1, 'max-content')).join(' ');
		return `${titleTrack} ${dataTracks}`;
	});
</script>

{#if totalNodes === 0}
	<EmptyHint />
{/if}

<div class="scroll-container" class:empty={totalNodes === 0} role="region" aria-label="Tree view">
	<div class="tree" role="treegrid" style="grid-template-columns: {gridTemplate};">
		{#if data.columns.length > 0}
			<div class="row header" role="row">
				<div class="cell title head-cell" role="columnheader">
					{prettifyId(data.field)}
					<ColumnResizeHandle columnIndex={0} widths={columnWidths} />
				</div>
				{#each data.columns as column, index (column.name)}
					<div class="cell head-cell" role="columnheader">
						{prettifyId(column.name)}
						{#if index < data.columns.length - 1}
							<ColumnResizeHandle columnIndex={index + 1} widths={columnWidths} />
						{/if}
					</div>
				{/each}
			</div>
		{/if}

		{#each data.roots as root (root.card.id)}
			<TreeNodeRow node={root} columns={data.columns} depth={0} {expandedIds} {toggle} />
		{/each}
	</div>
</div>

<RowCount count={totalNodes} />

<style>
	.scroll-container {
		overflow-x: auto;
		flex: 1;
		min-height: 0;
		border: 1px solid var(--color-border);
		border-radius: var(--radius-md);
		background-color: var(--color-bg);
	}

	.scroll-container.empty {
		display: none;
	}

	.tree {
		display: grid;
		width: 100%;
	}

	:global(.tree > .row) {
		display: contents;
	}

	:global(.tree .cell) {
		padding: var(--space-2) var(--space-3);
		border-bottom: 1px solid var(--color-border);
		background-color: var(--color-bg);
		vertical-align: top;
		min-width: 0;
	}

	/* Body data cells clip to the configured column width with ellipsis.
	   Header cells stay overflow-visible so resize handles can sit on
	   the edge. Body title cells keep their own flex layout — id and
	   title-text handle their own truncation. */
	:global(.tree .row:not(.header) .cell:not(.title)) {
		overflow: hidden;
		text-overflow: ellipsis;
		white-space: nowrap;
	}

	:global(.tree .cell.title) {
		position: sticky;
		left: 0;
		z-index: 1;
		display: flex;
		align-items: baseline;
		gap: var(--space-2);
		padding-left: calc(var(--space-3) + var(--indent, 0rem));
	}

	:global(.tree .cell.title::after) {
		content: '';
		position: absolute;
		top: 0;
		right: -1px;
		bottom: 0;
		width: 1px;
		background-color: var(--color-border);
	}

	/* Color-field treatment, identical to the table: rows are
	   `display: contents`, so the wash lands on each cell (the custom
	   property inherits through the row wrapper) and the full-strength
	   stripe is an inset shadow on the sticky title cell — no width
	   added, columns stay aligned, and the tint stays opaque while the
	   hierarchy column is stuck during horizontal scroll. */
	:global(.tree .row.tinted .cell) {
		background-color: color-mix(in srgb, var(--item-color) var(--tint-strength), var(--color-bg));
	}

	:global(.tree .row.tinted .cell.title) {
		box-shadow: inset 4px 0 0 0 var(--item-color);
	}

	:global(.tree .row.header .cell) {
		position: sticky;
		top: 0;
		background-color: var(--color-surface);
		font-weight: 600;
		font-size: var(--text-sm);
		color: var(--color-fg-muted);
		z-index: 2;
	}

	:global(.tree .row.header .cell.title) {
		z-index: 3;
	}
</style>
