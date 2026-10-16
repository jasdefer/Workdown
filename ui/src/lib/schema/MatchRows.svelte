<!--
  The `match:` block of a rule as rows: which items the rule applies
  to. Each row is a reference (relation and field), a quantifier when
  the reference goes through a many-valued relation, a test, and the
  test's value. The tests a row offers follow what its reference points
  at; the value is edited by the shared value editor with the target
  field's own spec, so a choice is picked from its values and a date
  from a date input.

  Owns no draft state: the rows are the value and every change goes up
  through `onchange` as the whole new list. Rows on the same reference
  are one condition in the file; the server merges them.
-->
<script lang="ts">
	import type { FieldDefinitionData } from '$lib/api/generated/FieldDefinitionData';
	import type { FieldValue } from '$lib/api/generated/FieldValue';
	import type { MatchRow } from '$lib/api/generated/MatchRow';
	import type { PaletteColor } from '$lib/api/generated/PaletteColor';
	import type { Quantifier } from '$lib/api/generated/Quantifier';
	import type { ResourceOption } from '$lib/api/generated/ResourceOption';
	import ValueEditor from '$lib/values/ValueEditor.svelte';
	import ReferencePicker from './ReferencePicker.svelte';
	import {
		emptyMatchRow,
		choiceOfTest,
		listSpecOfField,
		matchTestChoices,
		needsQuantifier,
		referenceKind,
		scalarOfValue,
		scalarsOfValue,
		specOfField,
		splitReference,
		targetField,
		valueOfScalar,
		valueOfScalars,
		withMatchReference,
		testOfChoice,
		type Anchor,
		type MatchTestChoice,
		type Reference
	} from './ruleDraft';

	interface Props {
		rows: MatchRow[];
		anchors: Anchor[];
		fields: FieldDefinitionData[];
		/** All item ids, for a `link` target's picker. */
		items: string[];
		palette: PaletteColor[];
		/** The entries of a resource list by name, for a resource-backed target. */
		optionsOfResource: (resource: string | null) => ResourceOption[];
		disabled?: boolean;
		onchange: (rows: MatchRow[]) => void;
	}

	let {
		rows,
		anchors,
		fields,
		items,
		palette,
		optionsOfResource,
		disabled = false,
		onchange
	}: Props = $props();

	// Exhaustive by type: a quantifier or test added in Rust reaches the
	// TypeScript union and fails the build here until it has a label.
	const QUANTIFIER_LABELS: Record<Quantifier, string> = {
		all: 'all of them',
		any: 'any of them',
		none: 'none of them'
	};
	const TEST_LABELS: Record<MatchTestChoice, string> = {
		equals: 'equals',
		one_of: 'is one of',
		not: 'is not',
		is_set: 'is set',
		is_not_set: 'is not set'
	};
	const quantifiers = Object.keys(QUANTIFIER_LABELS) as Quantifier[];

	function replace(index: number, row: MatchRow): void {
		onchange(rows.map((existing, position) => (position === index ? row : existing)));
	}

	function remove(index: number): void {
		onchange(rows.filter((_, position) => position !== index));
	}

	function add(): void {
		onchange([...rows, emptyMatchRow()]);
	}
</script>

<fieldset class="rows">
	<legend class="label">Applies to items where</legend>
	{#if rows.length === 0}
		<span class="hint">Every item. Add a condition to narrow it down.</span>
	{/if}
	{#each rows as row, index (index)}
		{@const reference = splitReference(row.reference, fields, anchors)}
		{@const kind = referenceKind(reference, anchors)}
		{@const target = targetField(reference, fields)}
		{@const label = `Condition ${String(index + 1)}`}
		<div class="row">
			<ReferencePicker
				{reference}
				{anchors}
				{fields}
				{label}
				{disabled}
				onchange={(next: Reference) => {
					replace(index, withMatchReference(row, next, anchors));
				}}
			/>
			{#if needsQuantifier(kind)}
				<select
					class="quantifier"
					aria-label="{label}: how many related items"
					value={row.quantifier ?? 'any'}
					{disabled}
					onchange={(event: Event & { currentTarget: HTMLSelectElement }) => {
						replace(index, { ...row, quantifier: event.currentTarget.value as Quantifier });
					}}
				>
					{#each quantifiers as quantifier (quantifier)}
						<option value={quantifier}>{QUANTIFIER_LABELS[quantifier]}</option>
					{/each}
				</select>
			{/if}
			<select
				class="test"
				aria-label="{label}: test"
				value={choiceOfTest(row.test)}
				disabled={disabled || kind === 'incomplete'}
				onchange={(event: Event & { currentTarget: HTMLSelectElement }) => {
					replace(index, {
						...row,
						test: testOfChoice(event.currentTarget.value as MatchTestChoice)
					});
				}}
			>
				{#each matchTestChoices(kind) as choice (choice)}
					<option value={choice}>{TEST_LABELS[choice]}</option>
				{/each}
			</select>
			<div class="value">
				{#if kind === 'incomplete'}
					<span class="hint">Pick a field.</span>
				{:else if row.test.kind === 'equals'}
					<ValueEditor
						spec={specOfField(target)}
						value={valueOfScalar(row.test.value)}
						{items}
						{palette}
						resourceOptions={optionsOfResource(target?.resource ?? null)}
						{disabled}
						onchange={(value: FieldValue | null) => {
							replace(index, {
								...row,
								test: { kind: 'equals', value: scalarOfValue(value) ?? '' }
							});
						}}
					/>
				{:else if row.test.kind === 'one_of' || row.test.kind === 'not'}
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
	<button type="button" class="add" {disabled} onclick={add}>+ Add condition</button>
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

	select {
		padding: 0.25rem var(--space-2);
		background-color: var(--color-bg);
		color: var(--color-fg);
		border: 1px solid var(--color-border);
		border-radius: var(--radius-sm);
		font-size: var(--text-sm);
		min-width: 0;
	}

	.quantifier,
	.test {
		flex: 0 1 auto;
	}

	.value {
		flex: 1 1 10rem;
		min-width: 0;
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
