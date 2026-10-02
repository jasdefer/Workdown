<!--
  The item detail as a slide-over: the shared `SlideOver` chrome around
  the shared `ItemEditor`, with an open-standalone link in the header.
  Opened by `/views/:id?item=:itemId`; closing removes the query param
  (handled by the host page via `onclose`). `onmutate` lets the host
  refetch the underlying view after an edit.
-->
<script lang="ts">
	import ItemEditor from '$lib/items/ItemEditor.svelte';
	import SlideOver from '$lib/ui/SlideOver.svelte';

	interface Props {
		itemId: string;
		onclose: () => void;
		onmutate: () => void;
	}

	let { itemId, onclose, onmutate }: Props = $props();
</script>

<SlideOver label="Item detail" {onclose}>
	{#snippet header()}
		<a class="standalone" href="/items/{itemId}">Open standalone ↗</a>
	{/snippet}
	<ItemEditor {itemId} {onmutate} />
</SlideOver>

<style>
	.standalone {
		font-size: var(--text-sm);
		color: var(--color-fg-muted);
	}
</style>
