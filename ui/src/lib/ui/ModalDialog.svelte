<!--
  The centered modal chrome: a native <dialog> opened with showModal()
  — top layer, inert page behind it, Escape and focus return for free —
  around the host's content. What every centered dialog needs and none
  should repeat lives here: the element opens when mounted and closes
  on teardown, every way the reader can close it routes to `onclose`
  (Escape, a click on the backdrop; the host wires its own buttons to
  `onclose` directly), and the box is centered explicitly because the
  global reset zeroes the user agent's `margin: auto`. The host mounts
  it conditionally and unmounts it from `onclose`. The host's root
  element must fill the box: a click whose target is the dialog element
  itself is read as a click on the backdrop.

  The element never closes itself: Escape's native `cancel` is
  prevented and handed to `onclose`, so the host's state (a URL param,
  a store flag) stays the one truth and a closed box can never sit
  under state that says open — a navigation can be superseded by a
  concurrent reload, and then the dialog must simply still be there.
  The `close` listener is the fallback for a close the browser forces
  anyway (a second Escape without user activation in between); the
  teardown's own `close()` is told apart from it by `tearingDown`.

  Two sizes: `form` is as tall as its content, up to the window, for a
  dialog that is read top to bottom (the commit dialog); `workspace` is
  a fixed large box whose host scrolls inside it (the item detail).

  Not for `ConfirmDialog`: that one is a popover at measured
  coordinates with an invisible backdrop, a different thing built on
  the same element.
-->
<script lang="ts">
	import type { Snippet } from 'svelte';

	interface Props {
		/** The accessible name, when no element inside is the title. */
		label?: string;
		/** The id of the element that is the title. */
		labelledby?: string;
		onclose: () => void;
		size?: 'form' | 'workspace';
		children: Snippet;
	}

	let { label, labelledby, onclose, size = 'form', children }: Props = $props();

	let dialog = $state<HTMLDialogElement>();
	// Set once the component is on its way out; the `close` event that
	// the teardown's own `close()` fires is then not the reader's.
	let tearingDown = false;

	$effect(() => {
		if (dialog === undefined || dialog.open) return;
		const element = dialog;
		element.showModal();
		// Close before unmounting so the browser restores focus to the
		// trigger even when the host drops the component while open.
		return () => {
			tearingDown = true;
			element.close();
		};
	});

	// Escape: keep the element open and let the host close it.
	function oncancel(event: Event): void {
		event.preventDefault();
		onclose();
	}

	// A close the browser forced anyway, or the teardown's own `close()`
	// — only the first is the reader's.
	function onnativeclose(): void {
		if (!tearingDown) onclose();
	}

	function onclick(event: MouseEvent): void {
		if (event.target === dialog) onclose();
	}
</script>

<dialog
	bind:this={dialog}
	class:workspace={size === 'workspace'}
	aria-label={label}
	aria-labelledby={labelledby}
	{oncancel}
	onclose={onnativeclose}
	{onclick}
>
	{@render children()}
</dialog>

<style>
	dialog {
		position: fixed;
		inset: 0;
		margin: auto;
		width: min(44rem, calc(100vw - 2rem));
		max-height: calc(100vh - 2rem);
		padding: 0;
		/* The host's background and scroll area keep to the corners. */
		overflow: hidden;
		background-color: var(--color-card);
		color: var(--color-fg);
		border: 1px solid var(--color-border);
		border-radius: var(--radius-md);
		box-shadow: var(--shadow-sm);
	}

	dialog.workspace {
		width: min(72rem, calc(100vw - 2rem));
		height: min(90vh, 60rem);
	}

	/* Light enough that the page behind stays legible — the ring on the
	   opened item has to show through it. */
	dialog::backdrop {
		background: rgb(0 0 0 / 0.3);
	}
</style>
