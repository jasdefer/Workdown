<!--
  Small rounded label used for tag-like field values (Choice,
  Multichoice, List, Links) and for view-navigation links. Neutral
  background and rounded-full shape.

  Variants:
    - `active`      — filled accent background (selected nav link).
    - `interactive` — hover affordance (nav link on hover).
  Pass `label` for plain text, or `children` to compose richer content
  (e.g. a kind icon + label in the nav).
-->
<script lang="ts">
	import type { Snippet } from 'svelte';

	interface Props {
		label?: string;
		active?: boolean;
		interactive?: boolean;
		children?: Snippet;
	}

	let { label, active = false, interactive = false, children }: Props = $props();
</script>

<span class="chip" class:active class:interactive>
	{#if children}{@render children()}{:else}{label}{/if}
</span>

<style>
	.chip {
		display: inline-block;
		padding: 0.125rem var(--space-2);
		background-color: var(--color-surface);
		color: var(--color-fg);
		border: 1px solid var(--color-border);
		border-radius: var(--radius-full);
		font-size: var(--text-sm);
		line-height: 1.4;
		white-space: nowrap;
	}

	.chip.interactive:hover:not(.active) {
		border-color: var(--color-fg-muted);
		color: var(--color-fg);
	}

	.chip.active {
		background-color: var(--color-accent);
		color: var(--color-accent-fg);
		border-color: var(--color-accent);
	}
</style>
