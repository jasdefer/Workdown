<!--
  Single kanban card. Title (or prettified id) prominent, id badge
  muted top-right, optional Markdown body preview with a CSS mask
  fade-out for the "more below" hint.
-->
<script lang="ts">
	import type { Card } from '$lib/api/generated/Card';
	import { draggable } from '$lib/dnd/dnd';
	import { activateOnKey, openItem } from '$lib/items/itemLink';
	import { timerStore } from '$lib/stores/timer.svelte';
	import RecordingDot from '$lib/timer/RecordingDot.svelte';
	import Markdown from '$lib/ui/Markdown.svelte';
	import { cardLabel } from '$lib/views/prettify';

	interface Props {
		card: Card;
		/**
		 * Whether the card is a drag source. Defaults to off: the board is
		 * the only view with a drop target, so it is the only caller that
		 * turns this on — and even it leaves it off when the grouping field
		 * is multi-valued. The gantt, graph and treemap views reuse this
		 * component with nothing to drop onto.
		 */
		dragEnabled?: boolean;
	}

	let { card, dragEnabled = false }: Props = $props();

	const displayTitle = $derived(cardLabel(card));

	// Click (when not a drag) opens the detail panel via `?item=`. The
	// card can be draggable, so it can't be an anchor — navigate in JS.
	// Opening the item works either way; only the drag is switched off.
	function open(): void {
		openItem(card.id);
	}
</script>

<div
	class="card"
	class:tinted={card.background !== null}
	style:--item-color={card.background}
	use:draggable={{ id: card.id, enabled: dragEnabled }}
	role="button"
	tabindex="0"
	onclick={open}
	onkeydown={(event) => {
		activateOnKey(event, open);
	}}
>
	<header class="card-header">
		<span class="title">{displayTitle}</span>
		{#if timerStore.runningItemId === card.id}
			<RecordingDot />
		{/if}
		<span class="id" aria-label="Item id" title={card.id}>{card.id}</span>
	</header>
	{#if card.subtitle}
		<div class="subtitle">{card.subtitle}</div>
	{/if}
	{#if card.body.trim().length > 0}
		<div class="body">
			<Markdown content={card.body} compact />
		</div>
	{/if}
</div>

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
		cursor: pointer;
		text-align: left;
	}

	/* Stripe + tint: the item's resolved `color` field. The stripe
	   carries the hue at full strength; the wash comes from the global
	   `.tinted` recipe (base.css). Untinted cards keep the neutral
	   theme background. */
	.card.tinted {
		background-color: var(--tint-wash);
		border-left: 4px solid var(--item-color);
	}

	.card:hover {
		border-color: var(--color-fg-muted);
	}

	/* The stripe keeps its hue on hover — only the neutral sides go
	   muted, so the color doesn't blink off when reaching for a card. */
	.card.tinted:hover {
		border-left-color: var(--item-color);
	}

	.card:focus-visible {
		outline: 2px solid var(--color-fg-muted);
		outline-offset: 1px;
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

	.subtitle {
		color: var(--color-fg-muted);
		font-size: 0.85em;
		overflow-wrap: anywhere;
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
