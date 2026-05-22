<!--
  Single kanban card. Title (or prettified id) prominent, id badge
  muted top-right, optional Markdown body preview with a CSS mask
  fade-out for the "more below" hint.
-->
<script lang="ts">
	import type { Card } from '$lib/api/generated/Card';
	import Markdown from '$lib/ui/Markdown.svelte';
	import { prettifyId } from '$lib/views/prettify';

	interface Props {
		card: Card;
	}

	let { card }: Props = $props();

	const displayTitle = $derived(card.title ?? prettifyId(card.id));
</script>

<article class="card">
	<header class="card-header">
		<span class="title">{displayTitle}</span>
		<span class="id" aria-label="Item id" title={card.id}>{card.id}</span>
	</header>
	{#if card.body.trim().length > 0}
		<div class="body">
			<Markdown content={card.body} compact />
		</div>
	{/if}
</article>

<style>
	.card {
		background-color: var(--color-bg);
		border: 1px solid var(--color-border);
		border-radius: var(--radius-md);
		padding: var(--space-3);
		box-shadow: var(--shadow-sm);
		display: flex;
		flex-direction: column;
		gap: var(--space-2);
	}

	.card-header {
		display: flex;
		justify-content: space-between;
		gap: var(--space-2);
		align-items: baseline;
	}

	.title {
		font-weight: 600;
		line-height: 1.3;
		flex: 1;
		min-width: 0;
		overflow-wrap: anywhere;
	}

	.id {
		font-family: var(--font-mono);
		color: var(--color-fg-muted);
		font-size: 0.75em;
		flex-shrink: 0;
		max-width: 8rem;
		overflow: hidden;
		text-overflow: ellipsis;
		white-space: nowrap;
	}

	.body {
		position: relative;
		max-height: 4.5rem;
		overflow: hidden;
		color: var(--color-fg-muted);
		-webkit-mask-image: linear-gradient(to bottom, black 60%, transparent 100%);
		mask-image: linear-gradient(to bottom, black 60%, transparent 100%);
	}
</style>
