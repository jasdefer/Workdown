<!--
  Table view. Semantic HTML <table> wrapped in a horizontally
  scrollable container. The header row is sticky at the top during
  vertical scroll; the first column is sticky at the left during
  horizontal scroll so the row's identity stays visible while
  scanning across.

  Click a column header to cycle its sort: asc → desc → unsorted.
  Sort state is session-local — the persistence question (URL
  param vs localStorage vs views.yaml) lands with the filter-editor
  and display-config issues, since all three share the same shape.

  Per-column resize: drag the right edge of any header (except the
  last) to set a fixed pixel width. On first drag we seed `columnWidths`
  with every column's currently rendered width and switch the table to
  `table-layout: fixed` so subsequent drags update only the dragged
  column without reflowing the others. Same persistence story as sort
  — session-local for now, joins view-display-config later.

  Per-cell formatting (dates, booleans, chips, link resolution) is
  delegated to Cell.svelte — TableView is just structure plus sort
  plus resize.
-->
<script lang="ts">
	import type { FieldType } from '$lib/api/generated/FieldType';
	import type { FieldValue } from '$lib/api/generated/FieldValue';
	import type { Column } from '$lib/api/generated/Column';
	import type { TableData } from '$lib/api/generated/TableData';
	import type { WorkItemId } from '$lib/api/generated/WorkItemId';
	import { SvelteMap } from 'svelte/reactivity';
	import { itemHref } from '$lib/items/itemLink';
	import { itemRefLabel, prettifyId } from '$lib/views/prettify';
	import ColumnResizeHandle from '$lib/views/ColumnResizeHandle.svelte';
	import EmptyHint from '$lib/views/EmptyHint.svelte';
	import RowCount from '$lib/views/RowCount.svelte';
	import Cell from './Cell.svelte';

	interface Props {
		data: TableData;
	}

	let { data }: Props = $props();

	let columnWidths = $state(new SvelteMap<number, number>());
	const isResizing = $derived(columnWidths.size > 0);

	// Run before the first resize: capture every column's current
	// rendered width so switching to table-layout: fixed doesn't
	// redistribute remaining columns to equal shares.
	function seedAllWidths(handle: HTMLElement) {
		if (columnWidths.size > 0) return;
		const thead = handle.closest('thead');
		if (!thead) return;
		const headerCells = thead.querySelectorAll('th');
		headerCells.forEach((th, index) => {
			columnWidths.set(index, th.getBoundingClientRect().width);
		});
	}

	function widthStyle(index: number): string {
		const width = columnWidths.get(index);
		return width === undefined ? '' : `width: ${width.toString()}px;`;
	}

	interface RenderableCell {
		column: Column;
		value: FieldValue | null;
	}

	interface RenderableRow {
		id: WorkItemId;
		/** Resolved `#rrggbb` of the item's first color field, or null. */
		background: string | null;
		cells: RenderableCell[];
	}

	const renderableRows = $derived<RenderableRow[]>(
		data.rows.map((row) => ({
			id: row.id,
			background: row.background,
			cells: data.columns.map((column, index) => ({
				column,
				value: row.cells[index] ?? null
			}))
		}))
	);

	type SortDirection = 'asc' | 'desc';
	type SortState = { columnName: string; direction: SortDirection } | null;

	let sort = $state<SortState>(null);

	function cycleSort(columnName: string) {
		if (sort?.columnName !== columnName) {
			sort = { columnName, direction: 'asc' };
		} else if (sort.direction === 'asc') {
			sort = { columnName, direction: 'desc' };
		} else {
			sort = null;
		}
	}

	function ariaSort(columnName: string): 'ascending' | 'descending' | 'none' {
		if (sort?.columnName !== columnName) return 'none';
		return sort.direction === 'asc' ? 'ascending' : 'descending';
	}

	function indicator(columnName: string): string {
		if (sort?.columnName !== columnName) return '';
		return sort.direction === 'asc' ? '↑' : '↓';
	}

	function linkLabel(id: WorkItemId): string {
		return itemRefLabel(data.items, id);
	}

	// Compare two non-null field values per the column's FieldType.
	// Strings/dates/choices use locale compare; numerics subtract;
	// list-shaped values join and compare; link/links resolve via the
	// items sidecar so the sort order matches what the user sees.
	// Duration sorts by its formatted string for now — fine for
	// similar-magnitude values, mis-orders mixed magnitudes ("10d" vs
	// "2d"). The wire shape doesn't carry the raw seconds; revisit if
	// it bites.
	function compareNonNull(a: FieldValue, b: FieldValue, fieldType: FieldType): number {
		switch (fieldType) {
			case 'integer':
			case 'float':
				return Number(a) - Number(b);
			case 'boolean':
				return Number(a) - Number(b);
			case 'string':
			case 'choice':
			case 'date':
			case 'duration':
			case 'color':
				return String(a).localeCompare(String(b));
			case 'multichoice':
			case 'list':
				return (a as string[]).join(',').localeCompare((b as string[]).join(','));
			case 'link':
				return linkLabel(a as WorkItemId).localeCompare(linkLabel(b as WorkItemId));
			case 'links': {
				const aLabels = (a as WorkItemId[]).map(linkLabel).join(',');
				const bLabels = (b as WorkItemId[]).map(linkLabel).join(',');
				return aLabels.localeCompare(bLabels);
			}
		}
	}

	const sortedRows = $derived.by<RenderableRow[]>(() => {
		const currentSort = sort;
		if (currentSort === null) return renderableRows;
		const columnIndex = data.columns.findIndex((column) => column.name === currentSort.columnName);
		if (columnIndex < 0) return renderableRows;
		const sortColumn = data.columns[columnIndex];
		if (!sortColumn) return renderableRows;
		const direction = currentSort.direction === 'asc' ? 1 : -1;

		// Slice to keep the source derived array intact. JS sort is
		// stable, so equal values preserve the backend's id-ascending
		// order — the natural secondary sort.
		return renderableRows.slice().sort((rowA, rowB) => {
			const cellA = rowA.cells[columnIndex];
			const cellB = rowB.cells[columnIndex];
			if (!cellA || !cellB) return 0;
			const valueA = cellA.value;
			const valueB = cellB.value;
			// Nulls always sort last regardless of direction.
			if (valueA === null && valueB === null) return 0;
			if (valueA === null) return 1;
			if (valueB === null) return -1;
			return compareNonNull(valueA, valueB, sortColumn.field_type) * direction;
		});
	});
</script>

{#if data.rows.length === 0}
	<EmptyHint />
{/if}

<div class="scroll-container" role="region" aria-label="Table view">
	<table class:fixed-layout={isResizing}>
		<thead>
			<tr>
				{#each data.columns as column, index (column.name)}
					<th aria-sort={ariaSort(column.name)} style={widthStyle(index)}>
						<button
							type="button"
							class="header-button"
							onclick={() => {
								cycleSort(column.name);
							}}
						>
							<span class="header-label">{prettifyId(column.name)}</span>
							<span class="header-indicator" aria-hidden="true">{indicator(column.name)}</span>
						</button>
						{#if index < data.columns.length - 1}
							<ColumnResizeHandle
								columnIndex={index}
								widths={columnWidths}
								onBeforeStart={seedAllWidths}
							/>
						{/if}
					</th>
				{/each}
			</tr>
		</thead>
		<tbody>
			{#each sortedRows as row (row.id)}
				<tr class:tinted={row.background !== null} style:--item-color={row.background}>
					{#each row.cells as cell, cellIndex (cell.column.name)}
						<td>
							{#if cellIndex === 0}
								<a class="row-link" href={itemHref(row.id)} title="Open {row.id}">
									<Cell value={cell.value} fieldType={cell.column.field_type} items={data.items} />
								</a>
							{:else}
								<Cell value={cell.value} fieldType={cell.column.field_type} items={data.items} />
							{/if}
						</td>
					{/each}
				</tr>
			{/each}
		</tbody>
	</table>
</div>

<RowCount count={data.rows.length} />

<style>
	.scroll-container {
		overflow-x: auto;
		flex: 1;
		min-height: 0;
		border: 1px solid var(--color-border);
		border-radius: var(--radius-md);
		background-color: var(--color-bg);
	}

	table {
		border-collapse: separate;
		border-spacing: 0;
		width: 100%;
	}

	.row-link {
		color: inherit;
		text-decoration: none;
		cursor: pointer;
	}

	.row-link:hover {
		text-decoration: underline;
	}

	.row-link:focus-visible {
		outline: 2px solid var(--color-accent);
		outline-offset: -2px;
	}

	/* Engaged once any column has a user-set width. Forces strict
	   width honoring on <th> and clips overflowing cell content with
	   ellipsis on body cells. */
	table.fixed-layout {
		table-layout: fixed;
	}

	table.fixed-layout tbody td {
		overflow: hidden;
		text-overflow: ellipsis;
		white-space: nowrap;
	}

	th,
	td {
		padding: var(--space-2) var(--space-3);
		text-align: left;
		vertical-align: top;
		border-bottom: 1px solid var(--color-border);
		background-color: var(--color-bg);
	}

	thead th {
		position: sticky;
		top: 0;
		z-index: 2;
		background-color: var(--color-surface);
		font-size: var(--text-sm);
		color: var(--color-fg-muted);
		border-bottom: 1px solid var(--color-border);
		padding: 0;
	}

	.header-button {
		display: flex;
		align-items: center;
		justify-content: space-between;
		gap: var(--space-1);
		width: 100%;
		padding: var(--space-2) var(--space-3);
		background: none;
		border: none;
		color: inherit;
		font: inherit;
		font-weight: 600;
		text-align: left;
		cursor: pointer;
	}

	.header-button:hover {
		color: var(--color-fg);
		background-color: var(--color-bg);
	}

	.header-button:focus-visible {
		outline: 2px solid var(--color-accent);
		outline-offset: -2px;
	}

	.header-indicator {
		color: var(--color-fg-muted);
		font-size: var(--text-sm);
		min-width: 1ch;
	}

	thead th[aria-sort='ascending'],
	thead th[aria-sort='descending'] {
		color: var(--color-fg);
	}

	tbody tr:last-child td {
		border-bottom: none;
	}

	/* Color-field treatment (mirrors the board card): a full-strength
	   stripe on the row's leading edge plus a --tint-strength wash across
	   its cells. The stripe is an inset shadow, not a border, so it adds
	   no width and tinted rows stay column-aligned with untinted ones.
	   The wash mixes into the theme background (adapts to light/dark);
	   the stripe hue is absolute, like a label color. The cell-level
	   selector out-specifies the base `td` background, and keeps the
	   sticky first column opaque so nothing shows through while scrolling. */
	tbody tr.tinted td {
		background-color: color-mix(in srgb, var(--item-color) var(--tint-strength), var(--color-bg));
	}

	tbody tr.tinted td:first-child {
		box-shadow: inset 4px 0 0 0 var(--item-color);
	}

	th:first-child,
	td:first-child {
		position: sticky;
		left: 0;
		z-index: 1;
	}

	thead th:first-child {
		z-index: 3;
	}

	th:first-child::after,
	td:first-child::after {
		content: '';
		position: absolute;
		top: 0;
		right: -1px;
		bottom: 0;
		width: 1px;
		background-color: var(--color-border);
	}
</style>
