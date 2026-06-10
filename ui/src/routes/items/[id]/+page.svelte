<script lang="ts">
	import ItemEditor from '$lib/items/ItemEditor.svelte';
	import type { PageData } from './$types';

	let { data }: { data: PageData } = $props();
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
		max-width: 42rem;
		margin: 0 auto;
		/* Counteract app-main's padding so the canvas reaches the edges,
		   then pad the content back in. */
		padding: var(--space-2) 0;
	}
</style>
