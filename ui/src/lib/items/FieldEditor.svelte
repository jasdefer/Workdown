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
	import type { PaletteColor } from '$lib/api/generated/PaletteColor';
	import Chip from '$lib/ui/Chip.svelte';
	import { prettifyId } from '$lib/views/prettify';

	interface Props {
		field: FieldSchema;
		value: FieldValue | null;
		/** All item ids — option set for link/links pickers. */
		items: string[];
		/** The built-in color palette — option set for color swatches. */
		palette?: PaletteColor[];
		disabled?: boolean;
		oncommit: (mutation: FieldMutation) => void;
	}

	let { field, value, items, palette = [], disabled = false, oncommit }: Props = $props();

	const asArray = $derived(Array.isArray(value) ? (value as string[]) : []);
	const asScalar = $derived(value === null ? '' : String(value));

	// The current color value resolved to hex — palette names resolve
	// through the served map, hex passes through. Drives the native
	// picker's value and the selected-swatch highlight (a stored hex
	// equal to a palette color selects that swatch too).
	const asHex = $derived.by(() => {
		if (asScalar === '') return null;
		if (asScalar.startsWith('#')) return asScalar;
		return palette.find((entry) => entry.name === asScalar)?.hex ?? null;
	});

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
{:else if field.field_type === 'color'}
	<div class="color-editor">
		{#each palette as entry (entry.name)}
			<!-- Clicking a swatch stores the *name* — the human-readable
			     authoring form that tracks future palette tuning. -->
			<button
				type="button"
				class="swatch"
				class:selected={asHex === entry.hex}
				style:background-color={entry.hex}
				title={entry.name}
				aria-label={entry.name}
				aria-pressed={asHex === entry.hex}
				{disabled}
				onclick={() => {
					commitScalar(entry.name, false);
				}}
			></button>
		{/each}
		<!-- The rainbow ring marks this as the any-color picker, so it
		     can't be mistaken for a ninth fixed swatch. -->
		<span class="picker-ring">
			<input
				type="color"
				class="picker"
				title="Pick a custom color"
				aria-label="Pick a custom color"
				value={asHex ?? '#808080'}
				{disabled}
				onchange={(event) => {
					commitScalar(event.currentTarget.value, false);
				}}
			/>
		</span>
		<!-- Free-text entry: a hex code or palette name, committed as
		     typed — the server validates (save-with-warning), same as
		     duration free text. Doubles as the current-value display. -->
		<input
			type="text"
			class="color-text"
			placeholder="#rrggbb or name"
			aria-label="Color as hex or palette name"
			value={asScalar}
			{disabled}
			onchange={(event) => {
				commitScalar(event.currentTarget.value.trim(), false);
			}}
		/>
		{#if asScalar !== '' && !field.required}
			<button
				type="button"
				class="remove"
				aria-label="Clear color"
				{disabled}
				onclick={() => {
					commitScalar('', false);
				}}>×</button
			>
		{/if}
	</div>
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

	.color-editor {
		display: flex;
		align-items: center;
		flex-wrap: wrap;
		gap: var(--space-1);
	}

	.swatch {
		width: 1.4rem;
		height: 1.4rem;
		padding: 0;
		border: 1px solid var(--color-border);
		border-radius: var(--radius-sm);
		cursor: pointer;
	}

	.swatch.selected {
		outline: 2px solid var(--color-fg);
		outline-offset: 1px;
	}

	.picker-ring {
		display: inline-flex;
		padding: 2px;
		border-radius: var(--radius-sm);
		background: conic-gradient(#ef4444, #eab308, #22c55e, #3b82f6, #a855f7, #ec4899, #ef4444);
	}

	.picker {
		width: 1.2rem;
		height: 1.2rem;
		padding: 0;
		border: none;
		border-radius: 2px;
		background: none;
		cursor: pointer;
	}

	input[type='text'].color-text {
		width: 8.5rem;
		flex: 0 0 auto;
		font-family: var(--font-mono);
		font-size: 0.8em;
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
