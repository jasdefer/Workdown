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

  Per-cell formatting (dates, booleans, chips, link resolution) is
  delegated to Cell.svelte — TableView is just structure plus sort.
-->
<script lang="ts">
	import type { FieldType } from '$lib/api/generated/FieldType';
	import type { FieldValue } from '$lib/api/generated/FieldValue';
	import type { TableColumn } from '$lib/api/generated/TableColumn';
	import type { TableData } from '$lib/api/generated/TableData';
	import type { WorkItemId } from '$lib/api/generated/WorkItemId';
	import { prettifyId } from '$lib/views/prettify';
	import Cell from './Cell.svelte';

	interface Props {
		data: TableData;
	}

	let { data }: Props = $props();

	interface RenderableCell {
		column: TableColumn;
		value: FieldValue | null;
	}

	interface RenderableRow {
		id: WorkItemId;
		cells: RenderableCell[];
	}

	const renderableRows = $derived<RenderableRow[]>(
		data.rows.map((row) => ({
			id: row.id,
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
		return data.items[id]?.title ?? prettifyId(id);
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

	const rowCountLabel = $derived(
		data.rows.length === 1 ? '1 item' : `${data.rows.length.toString()} items`
	);
</script>

{#if data.rows.length === 0}
	<p class="empty-hint">No items to display.</p>
{/if}

<div class="scroll-container" role="region" aria-label="Table view">
	<table>
		<thead>
			<tr>
				{#each data.columns as column (column.name)}
					<th aria-sort={ariaSort(column.name)}>
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
					</th>
				{/each}
			</tr>
		</thead>
		<tbody>
			{#each sortedRows as row (row.id)}
				<tr>
					{#each row.cells as cell (cell.column.name)}
						<td>
							<Cell value={cell.value} fieldType={cell.column.field_type} items={data.items} />
						</td>
					{/each}
				</tr>
			{/each}
		</tbody>
	</table>
</div>

{#if data.rows.length > 0}
	<p class="row-count">{rowCountLabel}</p>
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
