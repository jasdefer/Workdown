<!--
	Stub slide-over panel.

	The URL convention `/views/:id?item=:itemId` is settled; the visual
	form (slide-over from the right, Linear/Jira-style) is the leading
	candidate but defers until the real editing surface lands with
	first-view-end-to-end. For now this is just a marker that the view
	page noticed the query param.
-->

<script lang="ts">
	interface Props {
		itemId: string;
	}

	let { itemId }: Props = $props();
</script>

<aside class="panel">
	<header>
		<strong>Item: {itemId}</strong>
		<a href="/items/{itemId}">open standalone</a>
	</header>
	<p>The real item panel lands with the first interactive editing surface.</p>
</aside>

<style>
	.panel {
		position: fixed;
		top: 0;
		right: 0;
		bottom: 0;
		width: min(28rem, 100%);
		padding: var(--space-4);
		background-color: var(--color-surface);
		border-left: 1px solid var(--color-border);
		box-shadow: var(--shadow-sm);
		display: flex;
		flex-direction: column;
		gap: var(--space-3);
	}

	header {
		display: flex;
		align-items: center;
		justify-content: space-between;
		gap: var(--space-3);
	}

	p {
		color: var(--color-fg-muted);
		font-size: var(--text-sm);
	}
</style>
