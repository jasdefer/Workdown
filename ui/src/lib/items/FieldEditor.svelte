<!--
  Per-field editor, dispatched on the field's type — the write-side
  mirror of the read-side `Cell.svelte`. It owns no persistence: on a
  committed change it calls `oncommit` with a `FieldMutation`, and the
  host (`ItemEditor`) sends it and refreshes.

  Every edit is an absolute-value `replace` (or `unset` when an optional
  field is cleared) — collection fields send their whole new array. The
  append/remove/toggle ops exist on the wire for the CLI but the UI sets
  absolute values, which keeps each editor a plain controlled input.

  Editors read their current value straight from the `value` prop and
  commit on `change`; after the host refetches, the new prop flows back
  in. No local mirror state, so nothing can desync.
-->
<script lang="ts">
	import type { FieldMutation } from '$lib/api/generated/FieldMutation';
	import type { FieldSchema } from '$lib/api/generated/FieldSchema';
	import type { FieldValue } from '$lib/api/generated/FieldValue';
	import Chip from '$lib/ui/Chip.svelte';
	import { prettifyId } from '$lib/views/prettify';

	interface Props {
		field: FieldSchema;
		value: FieldValue | null;
		/** All item ids — option set for link/links pickers. */
		items: string[];
		disabled?: boolean;
		oncommit: (mutation: FieldMutation) => void;
	}

	let { field, value, items, disabled = false, oncommit }: Props = $props();

	const asArray = $derived(Array.isArray(value) ? (value as string[]) : []);
	const asScalar = $derived(value === null ? '' : String(value));

	function replace(next: unknown): void {
		oncommit({ op: 'replace', value: next });
	}

	function commitScalar(raw: string, numeric: boolean): void {
		// Clearing an optional field removes it; clearing a required one
		// still writes the empty value and lets the server warn.
		if (raw === '' && !field.required) {
			oncommit({ op: 'unset' });
			return;
		}
		if (numeric) {
			const parsed = Number(raw);
			if (!Number.isNaN(parsed)) replace(parsed);
			return;
		}
		replace(raw);
	}

	function toggleMember(option: string, checked: boolean): void {
		const next = new Set(asArray);
		if (checked) next.add(option);
		else next.delete(option);
		replace([...next]);
	}

	let draft = $state('');
	function addTag(): void {
		const tag = draft.trim();
		if (tag === '') return;
		replace([...asArray, tag]);
		draft = '';
	}
</script>

{#if field.field_type === 'boolean'}
	<input
		type="checkbox"
		checked={value === true}
		{disabled}
		onchange={(event) => {
			replace(event.currentTarget.checked);
		}}
	/>
{:else if field.field_type === 'choice'}
	<select
		{disabled}
		onchange={(event) => {
			commitScalar(event.currentTarget.value, false);
		}}
	>
		{#if !field.required}<option value="" selected={asScalar === ''}>—</option>{/if}
		{#each field.values ?? [] as option (option)}
			<option value={option} selected={asScalar === option}>{option}</option>
		{/each}
	</select>
{:else if field.field_type === 'multichoice'}
	<div class="options">
		{#each field.values ?? [] as option (option)}
			<label class="option">
				<input
					type="checkbox"
					checked={asArray.includes(option)}
					{disabled}
					onchange={(event) => {
						toggleMember(option, event.currentTarget.checked);
					}}
				/>
				{option}
			</label>
		{/each}
	</div>
{:else if field.field_type === 'date'}
	<input
		type="date"
		value={asScalar}
		{disabled}
		onchange={(event) => {
			commitScalar(event.currentTarget.value, false);
		}}
	/>
{:else if field.field_type === 'integer' || field.field_type === 'float'}
	<input
		type="number"
		step={field.field_type === 'integer' ? '1' : 'any'}
		min={field.min ?? undefined}
		max={field.max ?? undefined}
		value={asScalar}
		{disabled}
		onchange={(event) => {
			commitScalar(event.currentTarget.value, true);
		}}
	/>
{:else if field.field_type === 'link'}
	<select
		{disabled}
		onchange={(event) => {
			commitScalar(event.currentTarget.value, false);
		}}
	>
		<option value="" selected={asScalar === ''}>—</option>
		{#each items as id (id)}
			<option value={id} selected={asScalar === id}>{prettifyId(id)}</option>
		{/each}
	</select>
{:else if field.field_type === 'links'}
	<select
		multiple
		size={Math.min(Math.max(items.length, 2), 8)}
		{disabled}
		onchange={(event) => {
			replace([...event.currentTarget.selectedOptions].map((option) => option.value));
		}}
	>
		{#each items as id (id)}
			<option value={id} selected={asArray.includes(id)}>{prettifyId(id)}</option>
		{/each}
	</select>
{:else if field.field_type === 'list'}
	<div class="tags">
		{#each asArray as tag (tag)}
			<span class="tag">
				<Chip label={tag} />
				<button
					type="button"
					class="remove"
					aria-label={`Remove ${tag}`}
					{disabled}
					onclick={() => {
						replace(asArray.filter((entry) => entry !== tag));
					}}>×</button
				>
			</span>
		{/each}
	</div>
	<input
		type="text"
		placeholder="add value, press Enter"
		bind:value={draft}
		{disabled}
		onkeydown={(event) => {
			if (event.key === 'Enter') {
				event.preventDefault();
				addTag();
			}
		}}
	/>
{:else}
	<!-- string, duration, and resource-backed fields: free text. -->
	<input
		type="text"
		value={asScalar}
		placeholder={field.field_type === 'duration' ? 'e.g. 1w 2d' : ''}
		{disabled}
		onchange={(event) => {
			commitScalar(event.currentTarget.value, false);
		}}
	/>
{/if}

<style>
	input[type='text'],
	input[type='number'],
	input[type='date'],
	select {
		width: 100%;
		padding: 0.25rem var(--space-2);
		background-color: var(--color-bg);
		color: var(--color-fg);
		border: 1px solid var(--color-border);
		border-radius: var(--radius-sm);
		font-size: var(--text-sm);
	}

	.options {
		display: flex;
		flex-wrap: wrap;
		gap: var(--space-2);
	}

	.option {
		display: inline-flex;
		align-items: center;
		gap: 0.25rem;
		font-size: var(--text-sm);
	}

	.tags {
		display: flex;
		flex-wrap: wrap;
		gap: var(--space-1);
		margin-bottom: var(--space-1);
	}

	.tag {
		display: inline-flex;
		align-items: center;
		gap: 0.15rem;
	}

	.remove {
		background: none;
		border: none;
		color: var(--color-fg-muted);
		cursor: pointer;
		font-size: var(--text-sm);
		line-height: 1;
		padding: 0 0.15rem;
	}

	.remove:hover {
		color: var(--color-error-fg);
	}
</style>
