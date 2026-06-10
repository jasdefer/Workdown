<!--
  Kanban board view. Columns flex-fill the available width down to a
  280px minimum, then overflow horizontally. Synthetic `(none)` column
  is hidden when empty.

  Empty board (zero cards across all columns) → render the columns
  anyway so the structure stays visible, plus a quiet hint above.
-->
<script lang="ts">
	import { invalidateAll } from '$app/navigation';
	import { api } from '$lib/api/client';
	import type { BoardData } from '$lib/api/generated/BoardData';
	import type { FieldMutation } from '$lib/api/generated/FieldMutation';
	import EmptyHint from '$lib/views/EmptyHint.svelte';
	import Column from './Column.svelte';

	interface Props {
		data: BoardData;
	}

	let { data }: Props = $props();

	let actionError = $state<string | null>(null);

	const visibleColumns = $derived(
		data.columns.filter((column) => column.value !== null || column.cards.length > 0)
	);

	const totalCards = $derived(data.columns.reduce((sum, column) => sum + column.cards.length, 0));

	// Dropping a card sets the board field to the target column's value
	// (or unsets it on the synthetic "no value" column). On success we
	// refetch the view so computed columns/aggregates reflect the move;
	// the page's DiagnosticBanner then surfaces any save-with-warning.
	async function moveCard(cardId: string, toValue: string | null): Promise<void> {
		actionError = null;
		const mutation: FieldMutation =
			toValue === null ? { op: 'unset' } : { op: 'replace', value: toValue };
		const result = await api.setField(cardId, data.field, mutation);
		if (result.error !== undefined) {
			actionError = result.error;
			return;
		}
		await invalidateAll();
	}
</script>

{#if actionError}
	<p class="board-error" role="alert">{actionError}</p>
{/if}

{#if totalCards === 0}
	<EmptyHint />
{/if}

<div class="board" role="region" aria-label="Board view">
	{#each visibleColumns as column (column.value ?? '__synthetic__')}
		<Column
			{column}
			onmove={(cardId: string, toValue: string | null) => {
				void moveCard(cardId, toValue);
			}}
		/>
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

	.board-error {
		margin: 0 0 var(--space-2);
		padding: var(--space-2) var(--space-3);
		border: 1px solid var(--color-error-fg);
		border-radius: var(--radius-md);
		color: var(--color-error-fg);
		font-size: var(--text-sm);
	}
</style>
