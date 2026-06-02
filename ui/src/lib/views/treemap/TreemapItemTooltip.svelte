<!--
  Hover popover body for the treemap. Composes the rich board <Card>
  with a single size row above it so the reader sees both "what item
  is this" (title + id + body) and "how big is this rectangle"
  (the value in the size field) without having to scan the Card's
  configured fields for the right column.

  Pure composition — the board <Card> is reused untouched. Lift to a
  shared `<CardWithValue>` only if a second view needs the same shape.
-->
<script lang="ts">
	import type { Card as CardData } from '$lib/api/generated/Card';
	import Card from '$lib/views/board/Card.svelte';

	interface Props {
		card: CardData;
		sizeLabel: string;
		sizeFormatted: string;
		// Ancestor titles, outermost first. Rendered as a breadcrumb so the
		// reader can see which grouping chain this item sits in.
		chain: string[];
	}

	let { card, sizeLabel, sizeFormatted, chain }: Props = $props();
</script>

<div class="tooltip-body">
	{#if chain.length > 0}
		<p class="chain">
			{#each chain as crumb, index (index)}
				{#if index > 0}<span class="sep" aria-hidden="true">›</span>{/if}<span class="crumb"
					>{crumb}</span
				>
			{/each}
		</p>
	{/if}
	<dl class="size-row">
		<dt>{sizeLabel}</dt>
		<dd>{sizeFormatted}</dd>
	</dl>
	<Card {card} />
</div>

<style>
	.tooltip-body {
		display: flex;
		flex-direction: column;
		gap: var(--space-2);
	}

	.chain {
		margin: 0;
		font-size: var(--text-sm);
		color: var(--color-fg-muted);
		line-height: 1.3;
	}

	.chain .sep {
		margin: 0 0.35em;
		opacity: 0.7;
	}

	.size-row {
		display: flex;
		gap: var(--space-2);
		align-items: baseline;
		margin: 0;
		font-size: var(--text-sm);
	}

	.size-row dt {
		color: var(--color-fg-muted);
	}

	.size-row dt::after {
		content: ':';
	}

	.size-row dd {
		margin: 0;
		color: var(--color-fg);
		font-weight: 600;
		font-family: var(--font-mono);
	}
</style>
