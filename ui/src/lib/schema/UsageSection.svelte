<!--
  The "Used by" section of the field editor: what depends on the field,
  as the groups `summarizeUsage` sorts the server's answer into — rules,
  views, config roles, recipes, anything else — and the count of items
  holding a value. Display only: the panel asks the server and decides
  what the answer means for Remove (`blockedBy`); this shows it.
-->
<script lang="ts">
	import type { UsageState } from './fieldUsage';

	interface Props {
		usage: UsageState;
		/** What stands in the way of Remove, as a phrase; `null` when nothing does. */
		blockedBy: string | null;
	}

	let { usage, blockedBy }: Props = $props();

	// The named groups, in the order they are shown; an empty group is
	// left out.
	const namedGroups = $derived.by(() => {
		if (usage.status !== 'loaded') return [];
		const summary = usage.summary;
		return [
			{ label: 'Rules', names: summary.rules },
			{ label: 'Views', names: summary.views },
			{ label: 'Config roles', names: summary.roles },
			{ label: 'Recipes', names: summary.recipes }
		].filter((group) => group.names.length > 0);
	});
	const nothingDependsOnIt = $derived(
		usage.status === 'loaded' && blockedBy === null && usage.summary.itemCount === 0
	);
</script>

<section class="usage" aria-label="Used by">
	<span class="label">Used by</span>
	{#if usage.status === 'loading'}
		<span class="hint">Loading…</span>
	{:else if usage.status === 'failed'}
		<span class="hint">Could not load: {usage.message}</span>
	{:else if nothingDependsOnIt}
		<span class="hint">Nothing depends on this field.</span>
	{:else if usage.status === 'loaded'}
		{#each namedGroups as group (group.label)}
			<div class="group">
				<span class="group-label">{group.label}</span>
				<span class="group-names">
					{#each group.names as name, index (name)}{index > 0 ? ', ' : ''}<code>{name}</code>{/each}
				</span>
			</div>
		{/each}
		{#each usage.summary.other as message (message)}
			<p class="other">{message}</p>
		{/each}
		{#if usage.summary.itemCount > 0}
			<div class="group">
				<span class="group-label">Items</span>
				<span class="group-names">
					{usage.summary.itemCount === 1
						? '1 item holds a value'
						: `${String(usage.summary.itemCount)} items hold a value`}
				</span>
			</div>
		{/if}
		{#if blockedBy !== null}
			<span class="hint">Remove the field from the {blockedBy} first.</span>
		{/if}
	{/if}
</section>

<style>
	.usage {
		display: flex;
		flex-direction: column;
		gap: var(--space-1);
		padding: var(--space-3);
		border: 1px solid var(--color-border);
		border-radius: var(--radius-md);
		background-color: var(--color-surface);
	}

	.label {
		font-size: var(--text-sm);
		font-weight: 600;
		color: var(--color-fg-muted);
	}

	.hint {
		font-size: var(--text-sm);
		color: var(--color-fg-muted);
		margin: 0;
	}

	.group {
		display: grid;
		grid-template-columns: 6rem 1fr;
		gap: var(--space-2);
		font-size: var(--text-sm);
	}

	.group-label {
		color: var(--color-fg-muted);
	}

	.group-names {
		overflow-wrap: anywhere;
	}

	.group-names code {
		background-color: transparent;
		padding: 0;
	}

	/* A finding with no name to show: the message, wrapped, never cut. */
	.other {
		margin: 0;
		font-size: var(--text-sm);
		white-space: pre-wrap;
		overflow-wrap: anywhere;
		color: var(--color-fg-muted);
	}
</style>
