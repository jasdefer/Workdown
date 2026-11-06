<!--
  The header actions both item chromes share: the link to the
  standalone page and the switch between the two placements (modal or
  side panel). The switch flips the placement store; the view page,
  which picks its chrome from the store, then moves the open item into
  the other chrome at once — `?item=` is untouched, only where the item
  is shown changes. The close button is not here: it is the dialog's
  own or the docked panel's own.
-->
<script lang="ts">
	import { standaloneItemHref } from '$lib/items/itemLink';
	import { switchLabel } from '$lib/items/itemPlacement';
	import { placementStore } from '$lib/stores/placement.svelte';

	interface Props {
		itemId: string;
	}

	let { itemId }: Props = $props();
</script>

<div class="actions">
	<a class="standalone" href={standaloneItemHref(itemId)}>Open standalone ↗</a>
	<button
		type="button"
		class="switch"
		onclick={() => {
			placementStore.toggle();
		}}
	>
		{switchLabel(placementStore.value)}
	</button>
</div>

<style>
	.actions {
		display: flex;
		align-items: center;
		gap: var(--space-3);
		min-width: 0;
		font-size: var(--text-sm);
	}

	.standalone {
		color: var(--color-fg-muted);
	}

	/* Reads like the link beside it: a quiet action, not a form button. */
	.switch {
		background: none;
		border: none;
		padding: 0;
		font: inherit;
		color: var(--color-fg-muted);
		cursor: pointer;
		text-decoration: underline;
		text-underline-offset: 0.15em;
	}

	.standalone:hover,
	.switch:hover {
		color: var(--color-fg);
	}
</style>
