<!--
  Small rounded label used for tag-like field values (Choice,
  Multichoice, List, Links). Neutral background and rounded-full
  shape; semantic color variants land alongside the first surface
  that needs them.
-->
<script lang="ts">
	interface Props {
		label: string;
	}

	let { label }: Props = $props();
</script>

<span class="chip">{label}</span>

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
</style>
