<!--
  The item detail as a centered modal: a native <dialog> opened with
  showModal() — top layer, inert page behind it, Escape and focus
  return for free, the same chrome the commit dialog uses — around
  the shared `ItemEditor`, with an open-standalone link in the header.
  Opened by `/views/:id?item=:itemId`; closing removes the query param
  (the host page's `onclose`), so the dialog's own `close` event is
  routed there rather than closing the element directly — except the
  close the teardown itself issues: when the page navigates away (the
  open-standalone link), the element's `close` event still fires after
  the host is gone, and routing it would send the host's `onclose` to
  a view id that no longer exists. `onmutate` lets the host refetch the
  underlying view after an edit.

  The body is a size container: the editor lays its two areas out by
  the dialog's width, not the window's, so the standalone page and the
  dialog share one layout rule.
-->
<script lang="ts">
	import ItemEditor from '$lib/items/ItemEditor.svelte';

	interface Props {
		itemId: string;
		onclose: () => void;
		onmutate: () => void;
	}

	let { itemId, onclose, onmutate }: Props = $props();

	let dialog = $state<HTMLDialogElement>();
	// Set once the component is on its way out; the `close` event that
	// the teardown's own `close()` fires is then not the reader's.
	let tearingDown = false;

	$effect(() => {
		if (dialog === undefined || dialog.open) return;
		const element = dialog;
		element.showModal();
		return () => {
			tearingDown = true;
			element.close();
		};
	});

	// Escape (the native cancel → close) reaches here, and the teardown's
	// own `close()` — only the first is the reader closing the item.
	function onnativeclose(): void {
		if (!tearingDown) onclose();
	}

	function onclick(event: MouseEvent): void {
		// The box is fully covered by `.content`; a click on the dialog
		// element itself landed on the backdrop.
		if (event.target === dialog) onclose();
	}
</script>

<dialog bind:this={dialog} aria-label="Item detail" onclose={onnativeclose} {onclick}>
	<div class="content">
		<header>
			<a class="standalone" href="/items/{itemId}">Open standalone ↗</a>
			<button type="button" class="close" aria-label="Close item detail" onclick={onclose}>×</button
			>
		</header>
		<div class="body">
			<ItemEditor {itemId} {onmutate} />
		</div>
	</div>
</dialog>

<style>
	dialog {
		/* The global reset zeroes every margin, which takes away the user
		   agent's `margin: auto` that centers a modal dialog — so center
		   it explicitly. */
		position: fixed;
		inset: 0;
		margin: auto;
		width: min(72rem, calc(100vw - 2rem));
		height: min(90vh, 60rem);
		max-height: calc(100vh - 2rem);
		padding: 0;
		background-color: var(--color-canvas);
		color: var(--color-fg);
		border: 1px solid var(--color-border);
		border-radius: var(--radius-md);
		box-shadow: var(--shadow-sm);
	}

	/* Light enough that the ring on the opened item stays visible in
	   the view behind. */
	dialog::backdrop {
		background: rgb(0 0 0 / 0.3);
	}

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

	.body {
		flex: 1;
		min-height: 0;
		overflow-y: auto;
		padding: var(--space-4);
		container-type: inline-size;
	}
</style>
