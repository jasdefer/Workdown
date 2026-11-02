<!--
  Standalone item page: the shared `ItemEditor` in a wide, centered
  size container, so it lays out its two areas as the item dialog
  does. Visiting counts as opening the item: a view the reader goes
  to next marks it as the last opened.
-->
<script lang="ts">
	import ItemEditor from '$lib/items/ItemEditor.svelte';
	import { selectionStore } from '$lib/stores/selection.svelte';
	import type { PageData } from './$types';

	let { data }: { data: PageData } = $props();

	$effect(() => {
		selectionStore.setOpen(data.itemId);
		return () => {
			selectionStore.setOpen(null);
		};
	});
</script>

<section class="standalone">
	<div class="content">
		<ItemEditor itemId={data.itemId} />
	</div>
</section>

<style>
	.standalone {
		flex: 1;
		min-height: 0;
		overflow-y: auto;
		width: 100%;
		background: var(--color-canvas);
	}

	.content {
		max-width: 72rem;
		margin: 0 auto;
		container-type: inline-size;
		/* Counteract app-main's padding so the canvas reaches the edges,
		   then pad the content back in. */
		padding: var(--space-2) 0;
	}
</style>
