<!--
  Single-view page: renders the diagnostic banner above the view, then
  the `<ViewRenderer>` which dispatches on `data.result.data.type`.
  `?item=...` in the URL mounts the (stub) ItemPanel.
-->
<script lang="ts">
	import { goto, invalidateAll } from '$app/navigation';
	import type { PageData } from './$types';
	import DiagnosticBanner from '$lib/ui/DiagnosticBanner.svelte';
	import FilterBar from '$lib/filters/FilterBar.svelte';
	import DisplayBar from '$lib/views/DisplayBar.svelte';
	import ViewRenderer from '$lib/views/ViewRenderer.svelte';
	import ItemPanel from './ItemPanel.svelte';

	let { data }: { data: PageData } = $props();

	// Closing the panel drops `?item=` — load() depends on the query
	// param, so this re-runs and unmounts the panel.
	function closePanel(): void {
		void goto(`/views/${encodeURIComponent(data.viewId)}`, { keepFocus: true, noScroll: true });
	}
</script>

<div class="view-page">
	{#key data.viewId}
		<FilterBar viewId={data.viewId} initialFilter={data.filter} itemId={data.itemId} />
		<DisplayBar viewId={data.viewId} initialOverride={data.displayOverride} />
	{/key}

	<DiagnosticBanner
		diagnostics={data.result.diagnostics}
		viewData={data.result.data}
		currentViewId={data.viewId}
	/>

	{#if data.result.data}
		<div class="view-body">
			<ViewRenderer data={data.result.data} />
		</div>
	{:else}
		<div class="view-empty">
			<p>This view can't render. See the diagnostics above for details.</p>
		</div>
	{/if}
</div>

{#if data.itemId}
	<ItemPanel itemId={data.itemId} onclose={closePanel} onmutate={() => invalidateAll()} />
{/if}

<style>
	.view-page {
		display: flex;
		flex-direction: column;
		gap: var(--space-3);
		flex: 1;
		min-height: 0;
	}

	.view-body {
		flex: 1;
		min-height: 0;
		display: flex;
		flex-direction: column;
	}

	.view-empty {
		padding: var(--space-6);
		border: 1px dashed var(--color-border);
		border-radius: var(--radius-md);
		color: var(--color-fg-muted);
		text-align: center;
	}
</style>
