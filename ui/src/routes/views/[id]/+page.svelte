<!--
  Single-view page: a toolbar with the view's authoring actions, the
  diagnostic banner, then the `<ViewRenderer>` which dispatches on
  `data.result.data.type`. `?item=...` in the URL opens that item's
  detail and makes it the selected one, so the view marks it. Where the
  detail opens is the reader's placement: the centered modal
  (`ItemDialog`) or a panel docked beside the whole page (`ItemPanel`),
  switched from the detail's own header.

  The item param is read here, not in `load()`: opening or closing an
  item changes only the query param, and a load that depended on it
  would refetch and re-render the whole view before the detail even
  started loading. The view's load depends on the view id, the filter
  and the display override only.
-->
<script lang="ts">
	import { goto, invalidateAll } from '$app/navigation';
	import { page } from '$app/state';
	import type { PageData } from './$types';
	import DiagnosticBanner from '$lib/ui/DiagnosticBanner.svelte';
	import FilterBar from '$lib/filters/FilterBar.svelte';
	import DisplayBar from '$lib/views/DisplayBar.svelte';
	import ViewRenderer from '$lib/views/ViewRenderer.svelte';
	import ViewToolbar from '$lib/views/ViewToolbar.svelte';
	import { placementStore } from '$lib/stores/placement.svelte';
	import { selectionStore } from '$lib/stores/selection.svelte';
	import ItemDialog from './ItemDialog.svelte';
	import ItemPanel from './ItemPanel.svelte';

	let { data }: { data: PageData } = $props();

	const itemId = $derived(page.url.searchParams.get('item'));

	// The open item follows the URL; leaving the page (or the param)
	// clears it, and the store keeps it as the last opened.
	$effect(() => {
		selectionStore.setOpen(itemId);
		return () => {
			selectionStore.setOpen(null);
		};
	});

	// Closing the detail drops `?item=`; the param is read above, so
	// this unmounts the chrome without touching the view's data.
	function closeDetail(): void {
		void goto(`/views/${encodeURIComponent(data.viewId)}`, { keepFocus: true, noScroll: true });
	}

	function onmutate(): void {
		void invalidateAll();
	}
</script>

<div class="view-layout">
	<div class="view-page">
		{#key data.viewId}
			<ViewToolbar viewId={data.viewId} />
			<FilterBar viewId={data.viewId} initialFilter={data.filter} {itemId} />
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

	{#if itemId && placementStore.value === 'side'}
		<ItemPanel {itemId} onclose={closeDetail} {onmutate} />
	{/if}
</div>

{#if itemId && placementStore.value === 'dialog'}
	<ItemDialog {itemId} onclose={closeDetail} {onmutate} />
{/if}

<style>
	/* A row: the page, and the docked item panel beside it when one is
	   open. The page takes what the panel leaves, so the view shrinks
	   and nothing is covered. */
	.view-layout {
		display: flex;
		gap: var(--space-4);
		flex: 1;
		min-height: 0;
	}

	.view-page {
		display: flex;
		flex-direction: column;
		gap: var(--space-3);
		flex: 1;
		min-width: 0;
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
