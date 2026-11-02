<!--
  Single-view page: a toolbar with the view's authoring actions, the
  diagnostic banner, then the `<ViewRenderer>` which dispatches on
  `data.result.data.type`. `?item=...` in the URL mounts the item dialog
  and makes that item the selected one, so the view marks it.
-->
<script lang="ts">
	import { goto, invalidateAll } from '$app/navigation';
	import type { PageData } from './$types';
	import DiagnosticBanner from '$lib/ui/DiagnosticBanner.svelte';
	import FilterBar from '$lib/filters/FilterBar.svelte';
	import DisplayBar from '$lib/views/DisplayBar.svelte';
	import ViewRenderer from '$lib/views/ViewRenderer.svelte';
	import ViewToolbar from '$lib/views/ViewToolbar.svelte';
	import { selectionStore } from '$lib/stores/selection.svelte';
	import ItemDialog from './ItemDialog.svelte';

	let { data }: { data: PageData } = $props();

	// The open item follows the URL; leaving the page (or the param)
	// clears it, and the store keeps it as the last opened.
	$effect(() => {
		selectionStore.setOpen(data.itemId);
		return () => {
			selectionStore.setOpen(null);
		};
	});

	// Closing the dialog drops `?item=` — load() depends on the query
	// param, so this re-runs and unmounts the dialog.
	function closeDialog(): void {
		void goto(`/views/${encodeURIComponent(data.viewId)}`, { keepFocus: true, noScroll: true });
	}
</script>

<div class="view-page">
	{#key data.viewId}
		<ViewToolbar viewId={data.viewId} />
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
	<ItemDialog itemId={data.itemId} onclose={closeDialog} onmutate={() => invalidateAll()} />
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
