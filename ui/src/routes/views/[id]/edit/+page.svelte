<!-- Edit-view page: the ViewForm seeded with the persisted definition. -->
<script lang="ts">
	import type { PageData } from './$types';
	import ViewForm from '$lib/views/ViewForm.svelte';

	let { data }: { data: PageData } = $props();
</script>

{#key data.viewId}
	<ViewForm
		initial={{
			id: data.viewId,
			definition: data.seed.definition,
			filter: data.seed.filter
		}}
	/>
{/key}
