<!--
  Single kanban column. Sticky header at the top, vertically
  scrollable card list below. The synthetic "no value" column renders
  with a muted header named by `noValueLabel`.
-->
<script lang="ts">
	import type { BoardColumn } from '$lib/api/generated/BoardColumn';
	import { dropTarget } from '$lib/dnd/dnd';
	import { noValueLabel } from '$lib/views/prettify';
	import Card from './Card.svelte';

	interface Props {
		column: BoardColumn;
		/** Board field name, used to name the synthetic no-value column. */
		field: string;
		/** Whether this board's cards are drag sources at all. */
		dragEnabled: boolean;
		/** Move a dropped card to this column's value (the board field). */
		onmove: (cardId: string, toValue: string | null) => void;
	}

	let { column, field, dragEnabled, onmove }: Props = $props();
</script>

<section
	class="column"
	class:synthetic={column.value === null}
	use:dropTarget={(cardId) => {
		onmove(cardId, column.value);
	}}
>
	<header class="header">
		<span class="value">{column.value ?? noValueLabel(field)}</span>
		<span class="count" aria-label="Card count">{column.cards.length}</span>
	</header>
	<div class="cards">
		{#each column.cards as card (card.id)}
			<Card {card} {dragEnabled} />
		{/each}
	</div>
</section>

<style>
	.column {
		flex: 1 1 0;
		min-width: 280px;
		display: flex;
		flex-direction: column;
		min-height: 0;
		background-color: var(--color-surface);
		border-radius: var(--radius-md);
		padding: var(--space-2);
	}

	.header {
		position: sticky;
		top: 0;
		background-color: var(--color-surface);
		display: flex;
		justify-content: space-between;
		align-items: center;
		padding: var(--space-2) var(--space-1);
		border-bottom: 1px solid var(--color-border);
		font-weight: 600;
		font-size: var(--text-sm);
	}

	.column.synthetic .value {
		color: var(--color-fg-muted);
		font-style: italic;
	}

	.count {
		color: var(--color-fg-muted);
		font-family: var(--font-mono);
		font-size: 0.85em;
		font-weight: normal;
	}

	.cards {
		flex: 1;
		overflow-y: auto;
		display: flex;
		flex-direction: column;
		gap: var(--space-2);
		padding: var(--space-2) var(--space-1);
	}
</style>
