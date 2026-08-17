<!--
  Single-view page: a toolbar with the view's authoring actions, the
  diagnostic banner, then the `<ViewRenderer>` which dispatches on
  `data.result.data.type`. `?item=...` in the URL mounts the (stub)
  ItemPanel.
-->
<script lang="ts">
	import { goto, invalidateAll } from '$app/navigation';
	import type { PageData } from './$types';
	import { api } from '$lib/api/client';
	import DiagnosticBanner from '$lib/ui/DiagnosticBanner.svelte';
	import FilterBar from '$lib/filters/FilterBar.svelte';
	import DisplayBar from '$lib/views/DisplayBar.svelte';
	import ViewRenderer from '$lib/views/ViewRenderer.svelte';
	import ItemPanel from './ItemPanel.svelte';

	let { data }: { data: PageData } = $props();

	let deleting = $state(false);
	let deleteError = $state<string | null>(null);

	// Closing the panel drops `?item=` — load() depends on the query
	// param, so this re-runs and unmounts the panel.
	function closePanel(): void {
		void goto(`/views/${encodeURIComponent(data.viewId)}`, { keepFocus: true, noScroll: true });
	}

	async function deleteView(): Promise<void> {
		if (!confirm(`Delete view '${data.viewId}' from views.yaml?`)) return;
		deleting = true;
		deleteError = null;
		const result = await api.deleteView(data.viewId);
		deleting = false;
		if (result.error !== undefined) {
			deleteError = result.error;
			return;
		}
		// The view is gone: leave its page and refresh the navigation list.
		await goto('/', { invalidateAll: true });
	}
</script>

<div class="view-page">
	<div class="view-toolbar">
		<a class="toolbar-action" href={`/views/${encodeURIComponent(data.viewId)}/edit`}>Edit view</a>
		<button type="button" class="toolbar-action danger" disabled={deleting} onclick={deleteView}>
			{deleting ? 'Deleting…' : 'Delete view'}
		</button>
	</div>

	{#if deleteError !== null}
		<p class="delete-error" role="alert">{deleteError}</p>
	{/if}

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

	.view-toolbar {
		display: flex;
		justify-content: flex-end;
		gap: var(--space-3);
	}

	.toolbar-action {
		background: none;
		border: none;
		padding: 0;
		color: var(--color-fg-muted);
		font-size: var(--text-sm);
		text-decoration: none;
		cursor: pointer;
	}

	.toolbar-action:hover {
		color: var(--color-fg);
		text-decoration: underline;
	}

	.toolbar-action.danger:hover {
		color: var(--color-error-fg);
	}

	.toolbar-action:disabled {
		opacity: 0.5;
		cursor: default;
	}

	.delete-error {
		color: var(--color-error-fg);
		background-color: var(--color-error-bg);
		padding: var(--space-2);
		border-radius: var(--radius-sm);
		font-size: var(--text-sm);
		margin: 0;
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
