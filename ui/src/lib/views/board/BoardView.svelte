<!--
  Kanban board view. Columns flex-fill the available width down to a
  280px minimum, then overflow horizontally. Synthetic `(none)` column
  is hidden when empty.

  Empty board (zero cards across all columns) → render the columns
  anyway so the structure stays visible, plus a quiet hint above.
-->
<script lang="ts">
	import type { BoardData } from '$lib/api/generated/BoardData';
	import EmptyHint from '$lib/views/EmptyHint.svelte';
	import Column from './Column.svelte';

	interface Props {
		data: BoardData;
	}

	let { data }: Props = $props();

	const visibleColumns = $derived(
		data.columns.filter((column) => column.value !== null || column.cards.length > 0)
	);

	const totalCards = $derived(data.columns.reduce((sum, column) => sum + column.cards.length, 0));
</script>

{#if totalCards === 0}
	<EmptyHint />
{/if}

<div class="board" role="region" aria-label="Board view">
	{#each visibleColumns as column (column.value ?? '__synthetic__')}
		<Column {column} />
	{/each}
</div>

<style>
	.board {
		display: flex;
		gap: var(--space-4);
		overflow-x: auto;
		flex: 1;
		min-height: 0;
		padding-bottom: var(--space-2);
	}
</style>
