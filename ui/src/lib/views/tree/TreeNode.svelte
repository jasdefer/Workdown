<!--
  One tree row, plus its (recursively rendered) visible descendants.

  Emits N+1 direct elements per visible node so they participate in
  the parent grid's column alignment:
    - title cell: caret/leaf marker + id badge + title
    - one data cell per column (FieldType-driven via shared Cell)

  When expanded (id present in expandedIds), recursively emits each
  child as another TreeNode. Leaves render a small bullet in place
  of the caret to keep the column visually flush across depth.

  Tree data renders no item-resolution sidecar (titles are
  eager-resolved on the wire), so Cell receives an empty `items` map.
-->
<script lang="ts">
	import type { Column } from '$lib/api/generated/Column';
	import type { TreeNode } from '$lib/api/generated/TreeNode';
	import { SvelteSet } from 'svelte/reactivity';
	import { itemHref } from '$lib/items/itemLink';
	import Cell from '$lib/views/table/Cell.svelte';
	import { cardLabel } from '$lib/views/prettify';
	import Self from './TreeNode.svelte';

	interface Props {
		node: TreeNode;
		columns: Column[];
		depth: number;
		expandedIds: SvelteSet<string>;
		toggle: (id: string) => void;
	}

	let { node, columns, depth, expandedIds, toggle }: Props = $props();

	const id = $derived(node.card.id);
	const expanded = $derived(expandedIds.has(id));
	const hasChildren = $derived(node.children.length > 0);
	const displayTitle = $derived(cardLabel(node.card));
	const indentStyle = $derived(`--indent: ${(depth * 1.25).toString()}rem`);
</script>

<div
	class="row"
	role="row"
	class:tinted={node.card.background !== null}
	style:--item-color={node.card.background}
>
	<div class="cell title" style={indentStyle}>
		{#if hasChildren}
			<button
				type="button"
				class="caret"
				onclick={() => {
					toggle(id);
				}}
				aria-expanded={expanded}
				aria-label={expanded ? 'Collapse' : 'Expand'}
			>
				{expanded ? '▾' : '▸'}
			</button>
		{:else}
			<span class="caret leaf" aria-hidden="true">·</span>
		{/if}
		<span class="id" title={id}>{id}</span>
		<a class="title-text" href={itemHref(id)}>{displayTitle}</a>
	</div>
	{#each columns as column, index (column.name)}
		<div class="cell">
			<Cell value={node.cells[index] ?? null} fieldType={column.field_type} items={{}} />
		</div>
	{/each}
</div>

{#if expanded}
	{#each node.children as child (child.card.id)}
		<Self node={child} {columns} depth={depth + 1} {expandedIds} {toggle} />
	{/each}
{/if}

<style>
	.caret {
		background: none;
		border: none;
		padding: 0;
		font: inherit;
		cursor: pointer;
		color: var(--color-fg-muted);
		width: 1.25rem;
		text-align: left;
		flex-shrink: 0;
	}

	.caret:hover {
		color: var(--color-fg);
	}

	.caret:focus-visible {
		outline: 2px solid var(--color-accent);
		outline-offset: 2px;
		border-radius: var(--radius-md);
	}

	.caret.leaf {
		cursor: default;
		text-align: center;
	}

	.id {
		font-family: var(--font-mono);
		color: var(--color-fg-muted);
		font-size: 0.75em;
		flex-shrink: 0;
		max-width: 10rem;
		overflow: hidden;
		text-overflow: ellipsis;
		white-space: nowrap;
	}

	.title-text {
		font-weight: 600;
		line-height: 1.3;
		min-width: 0;
		overflow-wrap: anywhere;
		color: inherit;
		text-decoration: none;
	}

	.title-text:hover {
		text-decoration: underline;
	}

	.title-text:focus-visible {
		outline: 2px solid var(--color-accent);
		outline-offset: 2px;
		border-radius: var(--radius-md);
	}
</style>
