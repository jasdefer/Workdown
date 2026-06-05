<!--
  Shared view footer: "<count> <noun>" (e.g. "5 items", "3 bars", "7
  working days"). The noun is per-view — it names what the view actually
  counts — while the format and pluralization live here. Renders nothing
  for an empty view (the empty hint shows instead), so callers can drop
  it in unconditionally.
-->
<script lang="ts">
	import { pluralize } from './format';

	interface Props {
		count: number;
		noun?: string;
	}

	let { count, noun = 'item' }: Props = $props();
</script>

{#if count > 0}
	<p class="row-count">{pluralize(count, noun)}</p>
{/if}

<style>
	.row-count {
		margin: var(--space-2) 0 0;
		font-size: var(--text-sm);
		color: var(--color-fg-muted);
	}
</style>
