<!--
  Renders a single table cell based on the column's FieldType.

  Type-driven formatting:
    string, integer, float, duration → plain text
    date                              → YYYY-MM-DD
    boolean                           → ✓ / ✗
    choice                            → single chip
    multichoice, list                 → one chip per value
    link                              → resolved title (items map), else raw id
    links                             → one chip per resolved/raw id

  Null cells render blank. The `items` resolution map is the table-
  level sidecar from the wire — present ids resolve to titles,
  absent ids are broken links and fall through to raw id text.
-->
<script lang="ts">
	import type { FieldType } from '$lib/api/generated/FieldType';
	import type { FieldValue } from '$lib/api/generated/FieldValue';
	import type { ItemRef } from '$lib/api/generated/ItemRef';
	import type { WorkItemId } from '$lib/api/generated/WorkItemId';
	import Chip from '$lib/ui/Chip.svelte';
	import { prettifyId } from '$lib/views/prettify';

	interface Props {
		value: FieldValue | null;
		fieldType: FieldType;
		items: Partial<Record<WorkItemId, ItemRef>>;
	}

	let { value, fieldType, items }: Props = $props();

	function linkLabel(id: WorkItemId): string {
		const resolved = items[id];
		if (resolved === undefined) {
			return id;
		}
		return resolved.title ?? prettifyId(id);
	}
</script>

{#if value === null}
	{''}
{:else if fieldType === 'boolean'}
	<span aria-label={value ? 'true' : 'false'}>{value ? '✓' : '✗'}</span>
{:else if fieldType === 'date'}
	{value}
{:else if fieldType === 'choice'}
	<Chip label={value as string} />
{:else if fieldType === 'multichoice' || fieldType === 'list'}
	<span class="chip-row">
		{#each value as string[] as item (item)}
			<Chip label={item} />
		{/each}
	</span>
{:else if fieldType === 'link'}
	{linkLabel(value as WorkItemId)}
{:else if fieldType === 'links'}
	<span class="chip-row">
		{#each value as WorkItemId[] as id (id)}
			<Chip label={linkLabel(id)} />
		{/each}
	</span>
{:else}
	{value}
{/if}

<style>
	.chip-row {
		display: inline-flex;
		flex-wrap: wrap;
		gap: var(--space-1);
	}
</style>
