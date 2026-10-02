<!--
  The side-panel chrome: a panel at the right edge of the content area
  with a header row (the host's links on the left, the close button on
  the right) and a scrolling body. The item detail and the schema field
  editor are both this panel around their own content; what opens and
  closes it is the host's URL state, so `onclose` is the host's to
  interpret.

  Two placements. An overlay slides over the page: anchored to
  `.app-main` (the layout marks it `position: relative`), not the
  viewport, so it fills the content area below the app header and the
  header and the timer pill's expanded panel stay reachable. A docked
  panel sits beside the page in a flex row the host lays out, and the
  page shrinks to the remaining width — for a page whose content should
  stay fully visible while the panel is open, like a table of fields.
-->
<script lang="ts">
	import type { Snippet } from 'svelte';

	interface Props {
		/** The accessible name of the panel. */
		label: string;
		onclose: () => void;
		/**
		 * How much of the content area the panel takes on a wide window:
		 * `narrow` for a column of fields, `wide` for a form with sections
		 * side by side. Either fills the window when it is narrower.
		 */
		size?: 'narrow' | 'wide';
		/**
		 * Over the page (the default), or beside it in a flex row the
		 * host lays out.
		 */
		placement?: 'overlay' | 'docked';
		/** The header's left side: a link, a title, or nothing. */
		header?: Snippet;
		children: Snippet;
	}

	let {
		label,
		onclose,
		size = 'narrow',
		placement = 'overlay',
		header,
		children
	}: Props = $props();
</script>

<aside
	class="panel"
	class:wide={size === 'wide'}
	class:docked={placement === 'docked'}
	aria-label={label}
>
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
	.panel {
		position: absolute;
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

	/* Half the content area on a wide window, never less than the
	   narrow panel, the whole window on a small one. */
	.panel.wide {
		width: min(max(28rem, 50%), 100%);
	}

	/* In the host's row: full height of the row, its width taken from
	   the row rather than from `.app-main`, nothing to float above. */
	.panel.docked {
		position: static;
		flex: 0 0 auto;
		align-self: stretch;
		min-height: 0;
		z-index: auto;
		box-shadow: none;
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
