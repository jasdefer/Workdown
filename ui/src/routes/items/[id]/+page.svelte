<script lang="ts">
	import type { PageData } from './$types';

	let { data }: { data: PageData } = $props();
</script>

<section>
	<h1>Item: {data.itemId}</h1>
	<p>Standalone item detail/edit lands with the first interactive editing surface.</p>
</section>

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
