<!--
  Shared empty-state hint: the muted line a view shows when it has
  nothing to render. The "No items to display." default fits every view
  that counts items; pass `message` for views that count something else
  (e.g. metric tiles). Callers own the surrounding `{#if isEmpty}` — what
  counts as empty is view-specific.
-->
<script lang="ts">
	interface Props {
		message?: string;
	}

	let { message = 'No items to display.' }: Props = $props();
</script>

<p class="empty-hint">{message}</p>

<style>
	.empty-hint {
		color: var(--color-fg-muted);
		font-size: var(--text-sm);
		margin: 0 0 var(--space-3);
	}
</style>
