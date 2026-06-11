<!--
  Slide-over panel: chrome (position, close, open-standalone) around the
  shared `ItemEditor`. Opened by `/views/:id?item=:itemId`; closing
  removes the query param (handled by the host page via `onclose`).
  `onmutate` lets the host refetch the underlying view after an edit.
-->
<script lang="ts">
	import ItemEditor from '$lib/items/ItemEditor.svelte';

	interface Props {
		itemId: string;
		onclose: () => void;
		onmutate: () => void;
	}

	let { itemId, onclose, onmutate }: Props = $props();
</script>

<aside class="panel" aria-label="Item detail">
	<header>
		<a class="standalone" href="/items/{itemId}">Open standalone ↗</a>
		<button type="button" class="close" aria-label="Close panel" onclick={onclose}>×</button>
	</header>
	<div class="panel-body">
		<ItemEditor {itemId} {onmutate} />
	</div>
</aside>

<style>
	.panel {
		position: fixed;
		top: 0;
		right: 0;
		bottom: 0;
		width: min(28rem, 100%);
		background-color: var(--color-surface);
		border-left: 1px solid var(--color-border);
		box-shadow: var(--shadow-sm);
		display: flex;
		flex-direction: column;
		z-index: 10;
	}

	header {
		display: flex;
		align-items: center;
		justify-content: space-between;
		gap: var(--space-3);
		padding: var(--space-2) var(--space-3);
		border-bottom: 1px solid var(--color-border);
	}

	.standalone {
		font-size: var(--text-sm);
		color: var(--color-fg-muted);
	}

	.close {
		background: none;
		border: none;
		font-size: var(--text-lg);
		line-height: 1;
		cursor: pointer;
		color: var(--color-fg-muted);
		padding: 0 var(--space-1);
	}

	.close:hover {
		color: var(--color-fg);
	}

	.panel-body {
		flex: 1;
		min-height: 0;
		overflow-y: auto;
		padding: var(--space-4);
		background: var(--color-canvas);
	}
</style>
