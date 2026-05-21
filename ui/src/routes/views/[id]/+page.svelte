<script lang="ts">
	import type { PageData } from './$types';
	import ItemPanel from './ItemPanel.svelte';

	let { data }: { data: PageData } = $props();
</script>

<section>
	<h1>View: {data.viewId}</h1>
	<p>Real rendering lands with the next slice.</p>
</section>

{#if data.itemId}
	<ItemPanel itemId={data.itemId} />
{/if}

<style>
	section {
		display: flex;
		flex-direction: column;
		gap: var(--space-2);
	}

	h1 {
		font-size: var(--text-lg);
		font-weight: 600;
	}

	p {
		color: var(--color-fg-muted);
	}
</style>
