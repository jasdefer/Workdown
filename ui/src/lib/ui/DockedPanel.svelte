<!--
  The docked panel chrome: a panel beside the page in a flex row the
  host lays out, with a header row (the host's links on the left, the
  close button on the right) and a scrolling body. The page shrinks to
  the remaining width, so its content stays fully visible while the
  panel is open — for a form whose source should stay in view, like
  the schema field editor beside the table of fields. What opens and
  closes it is the host's URL state, so `onclose` is the host's to
  interpret; every way the reader can close it routes there, as in
  `ModalDialog`: the close button, and Escape.

  There is no native dialog here, so Escape is wired by hand, once, for
  every host: while a panel is open, Escape anywhere on the page closes
  it — the reader works in the page beside the panel (a card, a row),
  so focus is rarely inside it. Not when something nearer already took
  the key: a listener that marks its Escape handled (the timer menus
  call `preventDefault`), or an open `<dialog>` (a confirmation
  popover), whose own Escape it is.

  The row itself is the host's: a flex row with the page as a shrinking
  `flex: 1; min-width: 0` column and this panel beside it, the same few
  lines on the schema page and the view page. Two copies were judged
  cheaper than a shared class for a rule this small; a third host makes
  it worth sharing.
-->
<script lang="ts">
	import type { Snippet } from 'svelte';

	interface Props {
		/** The accessible name of the panel. */
		label: string;
		onclose: () => void;
		/**
		 * How much of the row the panel takes on a wide window: `narrow`
		 * for a column of fields, `wide` for a form with sections side by
		 * side. Either fills the window when it is narrower.
		 */
		size?: 'narrow' | 'wide';
		/** The header's left side: a link, a title, or nothing. */
		header?: Snippet;
		children: Snippet;
	}

	let { label, onclose, size = 'narrow', header, children }: Props = $props();

	function onkeydown(event: KeyboardEvent): void {
		if (event.key !== 'Escape' || event.defaultPrevented) return;
		const target = event.target;
		if (target instanceof Element && target.closest('dialog[open]') !== null) return;
		event.preventDefault();
		onclose();
	}
</script>

<svelte:window {onkeydown} />

<aside class="panel" class:wide={size === 'wide'} aria-label={label}>
	<header>
		<div class="header-start">
			{#if header}{@render header()}{/if}
		</div>
		<button type="button" class="close" aria-label="Close panel" onclick={onclose}>×</button>
	</header>
	<div class="panel-body">
		{@render children()}
	</div>
</aside>

<style>
	/* Full height of the host's row, its width taken from the row. */
	.panel {
		flex: 0 0 auto;
		align-self: stretch;
		min-height: 0;
		width: min(28rem, 100%);
		background-color: var(--color-surface);
		border-left: 1px solid var(--color-border);
		display: flex;
		flex-direction: column;
	}

	/* Half the row on a wide window, never less than the narrow panel,
	   the whole window on a small one. */
	.panel.wide {
		width: min(max(28rem, 50%), 100%);
	}

	header {
		display: flex;
		align-items: center;
		justify-content: space-between;
		gap: var(--space-3);
		padding: var(--space-2) var(--space-3);
		border-bottom: 1px solid var(--color-border);
	}

	.header-start {
		display: flex;
		align-items: center;
		gap: var(--space-3);
		min-width: 0;
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

	/* A size container, so the content can lay itself out by the
	   panel's width rather than the window's. */
	.panel-body {
		flex: 1;
		min-height: 0;
		overflow-y: auto;
		padding: var(--space-4);
		background: var(--color-canvas);
		container-type: inline-size;
	}
</style>
