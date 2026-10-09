<!--
  The `require:` block of a rule as rows: what must hold on each item
  the rule applies to. Each row is a reference (relation and field), a
  requirement, and what the requirement needs — a list of values, a
  comparison with another field, a count. Which requirements a row
  offers follow what its reference points at and the target field's
  type, so a comparison is only offered on a date or a number and a
  count only on the related items themselves.

  Owns no draft state: the rows are the value and every change goes up
  through `onchange` as the whole new list.
-->
<script lang="ts">
	import type { Comparison } from '$lib/api/generated/Comparison';
	import type { FieldDefinitionData } from '$lib/api/generated/FieldDefinitionData';
	import type { FieldValue } from '$lib/api/generated/FieldValue';
	import type { PaletteColor } from '$lib/api/generated/PaletteColor';
	import type { RequireRow } from '$lib/api/generated/RequireRow';
	import type { RequireTest } from '$lib/api/generated/RequireTest';
	import ValueEditor from '$lib/values/ValueEditor.svelte';
	import ReferencePicker from './ReferencePicker.svelte';
	import {
		comparableFields,
		emptyRequireRow,
		emptyRequireTest,
		listSpecOfField,
		referenceKind,
		requireTestKinds,
		scalarsOfValue,
		splitReference,
		targetField,
		valueOfScalars,
		withRequireReference,
		type Anchor,
		type Reference
	} from './ruleDraft';

	interface Props {
		rows: RequireRow[];
		anchors: Anchor[];
		fields: FieldDefinitionData[];
		/** All item ids, for a `link` target's picker. */
		items: string[];
		palette: PaletteColor[];
		disabled?: boolean;
		onchange: (rows: RequireRow[]) => void;
	}

	let { rows, anchors, fields, items, palette, disabled = false, onchange }: Props = $props();

	// Exhaustive by type: a requirement or comparison added in Rust
	// reaches the TypeScript union and fails the build here until it has
	// a label.
	const TEST_LABELS: Record<RequireTest['kind'], string> = {
		required: 'must be set',
		forbidden: 'must not be set',
		values: 'must be one of',
		not: 'must not be',
		compare: 'must compare with',
		min_count: 'must number at least',
		max_count: 'must number at most'
	};
	const COMPARISON_LABELS: Record<Comparison, string> = {
		eq: '=',
		lt: '<',
		lte: '≤',
		gt: '>',
		gte: '≥'
	};
	const comparisons = Object.keys(COMPARISON_LABELS) as Comparison[];

	function replace(index: number, row: RequireRow): void {
		onchange(rows.map((existing, position) => (position === index ? row : existing)));
	}

	function remove(index: number): void {
		onchange(rows.filter((_, position) => position !== index));
	}

	function add(): void {
		onchange([...rows, emptyRequireRow()]);
	}

	function countOf(text: string): number {
		const parsed = Number.parseInt(text, 10);
		return Number.isNaN(parsed) || parsed < 0 ? 0 : parsed;
	}
</script>

<fieldset class="rows">
	<legend class="label">Then on each of them</legend>
	{#if rows.length === 0}
		<span class="hint">Nothing yet. Add a requirement, or limit the count below.</span>
	{/if}
	{#each rows as row, index (index)}
		{@const reference = splitReference(row.reference, fields, anchors)}
		{@const kind = referenceKind(reference, anchors)}
		{@const target = targetField(reference, fields)}
		{@const label = `Requirement ${String(index + 1)}`}
		<div class="row">
			<ReferencePicker
				{reference}
				{anchors}
				{fields}
				{label}
				{disabled}
				onchange={(next: Reference) => {
					const nextTarget = targetField(next, fields);
					replace(index, withRequireReference(row, next, anchors, nextTarget?.field_type ?? null));
				}}
			/>
			<select
				class="test"
				aria-label="{label}: requirement"
				value={row.test.kind}
				disabled={disabled || kind === 'incomplete'}
				onchange={(event: Event & { currentTarget: HTMLSelectElement }) => {
					replace(index, {
						...row,
						test: emptyRequireTest(event.currentTarget.value as RequireTest['kind'])
					});
				}}
			>
				{#each requireTestKinds(kind, target?.field_type ?? null) as testKind (testKind)}
					<option value={testKind}>{TEST_LABELS[testKind]}</option>
				{/each}
			</select>
			<div class="value">
				{#if kind === 'incomplete'}
					<span class="hint">Pick a field.</span>
				{:else if row.test.kind === 'values' || row.test.kind === 'not'}
					{@const testKind = row.test.kind}
					<ValueEditor
						spec={listSpecOfField(target)}
						value={valueOfScalars(row.test.values)}
						{items}
						{palette}
						{disabled}
						onchange={(value: FieldValue | null) => {
							replace(index, {
								...row,
								test: { kind: testKind, values: scalarsOfValue(value, target?.field_type ?? null) }
							});
						}}
					/>
				{:else if row.test.kind === 'compare'}
					{@const test = row.test}
					{@const candidates = target === null ? [] : comparableFields(fields, target)}
					{@const stray = test.field !== '' && !candidates.includes(test.field) ? test.field : null}
					<select
						class="comparison"
						aria-label="{label}: comparison"
						value={test.comparison}
						{disabled}
						onchange={(event: Event & { currentTarget: HTMLSelectElement }) => {
							replace(index, {
								...row,
								test: { ...test, comparison: event.currentTarget.value as Comparison }
							});
						}}
					>
						{#each comparisons as comparison (comparison)}
							<option value={comparison}>{COMPARISON_LABELS[comparison]}</option>
						{/each}
					</select>
					<select
						class="other"
						aria-label="{label}: field to compare with"
						value={test.field}
						{disabled}
						onchange={(event: Event & { currentTarget: HTMLSelectElement }) => {
							replace(index, { ...row, test: { ...test, field: event.currentTarget.value } });
						}}
					>
						<option value="">— field —</option>
						{#each candidates as candidate (candidate)}
							<option value={candidate}>{candidate}</option>
						{/each}
						{#if stray !== null}
							<option value={stray}>{stray} (unknown)</option>
						{/if}
					</select>
				{:else if row.test.kind === 'min_count' || row.test.kind === 'max_count'}
					{@const testKind = row.test.kind}
					<input
						type="number"
						class="count"
						aria-label="{label}: count"
						min="0"
						step="1"
						value={row.test.count}
						{disabled}
						onchange={(event: Event & { currentTarget: HTMLInputElement }) => {
							replace(index, {
								...row,
								test: { kind: testKind, count: countOf(event.currentTarget.value) }
							});
						}}
					/>
				{/if}
			</div>
			<button
				type="button"
				class="remove"
				aria-label="Remove {label}"
				title="Remove"
				{disabled}
				onclick={() => {
					remove(index);
				}}>×</button
			>
		</div>
	{/each}
	<button type="button" class="add" {disabled} onclick={add}>+ Add requirement</button>
</fieldset>

<style>
	.rows {
		display: flex;
		flex-direction: column;
		gap: var(--space-2);
		margin: 0;
		padding: 0;
		border: none;
		min-width: 0;
	}

	.label {
		font-size: var(--text-sm);
		font-weight: 600;
		color: var(--color-fg-muted);
		padding: 0;
	}

	.row {
		display: flex;
		flex-wrap: wrap;
		align-items: center;
		gap: var(--space-1);
		padding: var(--space-2);
		border: 1px solid var(--color-border);
		border-radius: var(--radius-sm);
		background-color: var(--color-surface);
	}

	select,
	input[type='number'] {
		padding: 0.25rem var(--space-2);
		background-color: var(--color-bg);
		color: var(--color-fg);
		border: 1px solid var(--color-border);
		border-radius: var(--radius-sm);
		font-size: var(--text-sm);
		min-width: 0;
	}

	.test,
	.comparison {
		flex: 0 1 auto;
	}

	.other {
		flex: 1 1 8rem;
		font-family: var(--font-mono);
	}

	.count {
		width: 5rem;
	}

	.value {
		flex: 1 1 10rem;
		min-width: 0;
		display: flex;
		align-items: center;
		gap: var(--space-1);
	}

	.remove,
	.add {
		flex: none;
		padding: 0.25rem var(--space-2);
		background-color: var(--color-surface);
		color: var(--color-fg-muted);
		border: 1px solid var(--color-border);
		border-radius: var(--radius-sm);
		font-size: var(--text-sm);
		line-height: 1;
		cursor: pointer;
	}

	.remove {
		width: 1.75rem;
		padding-left: 0;
		padding-right: 0;
	}

	.remove:not(:disabled):hover {
		color: var(--color-error-fg);
	}

	.add {
		align-self: flex-start;
	}

	.remove:disabled,
	.add:disabled {
		opacity: 0.5;
		cursor: default;
	}

	.hint {
		font-size: var(--text-sm);
		color: var(--color-fg-muted);
	}
</style>
