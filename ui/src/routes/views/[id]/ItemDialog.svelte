<!--
  The item detail as a centered modal: the shared `ModalDialog` chrome
  (workspace size) around the shared `ItemEditor`, with the shared
  header actions (standalone link, placement switch) and a close button
  in the header. Opened by `/views/:id?item=:itemId` when the reader's
  placement is the modal (`ItemPanel` is the other chrome); closing
  removes the query param (the host page's `onclose`), where Escape,
  the backdrop and the close button all route. `onmutate` lets the host
  refetch the underlying view after an edit.

  The body is a size container: the editor lays its two areas out by
  the dialog's width, not the window's, so the standalone page and the
  dialog share one layout rule.
-->
<script lang="ts">
	import ItemEditor from '$lib/items/ItemEditor.svelte';
	import { itemHref } from '$lib/items/itemLink';
	import ModalDialog from '$lib/ui/ModalDialog.svelte';
	import ItemDetailActions from './ItemDetailActions.svelte';

	interface Props {
		itemId: string;
		onclose: () => void;
		onmutate: () => void;
	}

	let { itemId, onclose, onmutate }: Props = $props();
</script>

<ModalDialog label="Item detail" size="workspace" {onclose}>
	<div class="content">
		<header>
			<ItemDetailActions {itemId} />
			<button type="button" class="close" aria-label="Close item detail" onclick={onclose}>×</button
			>
		</header>
		<div class="body">
			<!-- A reference followed from inside swaps the dialog's item. -->
			<ItemEditor {itemId} {onmutate} hrefFor={itemHref} />
		</div>
	</div>
</ModalDialog>

<style>
	.content {
		height: 100%;
		display: flex;
		flex-direction: column;
	}

	header {
		flex: none;
		display: flex;
		align-items: center;
		justify-content: space-between;
		gap: var(--space-3);
		padding: var(--space-2) var(--space-4);
		border-bottom: 1px solid var(--color-border);
		background-color: var(--color-surface);
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

	/* The canvas, not the card: the editor's own cards sit on it. */
	.body {
		flex: 1;
		min-height: 0;
		overflow-y: auto;
		padding: var(--space-4);
		background-color: var(--color-canvas);
		container-type: inline-size;
	}
</style>
