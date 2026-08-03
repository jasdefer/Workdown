<!--
  One guided filter condition: field → operator → value pickers.

  Reads the editing vocabulary from `schemaStore` (fields, per-type
  operators, item ids for link pickers). Owns no state — it renders the
  `row` prop and reports every edit up via `onchange` with a fresh row, so
  the parent (`FilterBuilder`) stays the single source of truth for the
  draft.
-->
<script lang="ts">
	import type { FieldType } from '$lib/api/generated/FieldType';
	import type { Operator } from '$lib/api/generated/Operator';
	import { schemaStore } from '$lib/stores/schema.svelte';
	import { prettifyId } from '$lib/views/prettify';
	import {
		isMultiValueOperator,
		isPresenceOperator,
		operatorLabel,
		withOperator,
		type GuidedRow
	} from './clauses';

	interface Props {
		row: GuidedRow;
		onchange: (row: GuidedRow) => void;
		onremove: () => void;
	}

	let { row, onchange, onremove }: Props = $props();

	const fieldDef = $derived(schemaStore.field(row.field));
	const fieldType = $derived<FieldType | undefined>(fieldDef?.field_type);
	// Offered operators for this field type, plus the row's current operator
	// if it isn't one of them — so a filter that was hand-written (or saved
	// before the offered set narrowed) still displays and stays editable,
	// rather than the select rendering blank and silently changing it.
	const operators = $derived<Operator[]>(
		withCurrentOperator(fieldType ? schemaStore.operatorsFor(fieldType) : [], row.operator)
	);
	const showValue = $derived(row.operator !== '' && !isPresenceOperator(row.operator));
	// `in` / `not in` are offered for the types with a known value set to
	// pick members from — but a hand-written clause carries them on any
	// field, so the widgets below pick by field type and fall back to
	// free-text members rather than guessing by elimination.
	const isMulti = $derived(isMultiValueOperator(row.operator));
	const picksFromChoices = $derived(fieldType === 'choice' || fieldType === 'multichoice');
	const isRelation = $derived(fieldType === 'link' || fieldType === 'links');
	// A `resource:`-backed field filters by the stored id, so the picker
	// offers labels and sets ids — the same join the item editor makes.
	// Empty when the section is missing or has no entries, which falls the
	// row back to free text.
	const resourceOptions = $derived(schemaStore.resourceOptions(fieldDef));
	const selectedValues = $derived(row.values);
	const scalarValue = $derived(row.value ?? '');

	// The closed option set the row's widget offers, or null when the
	// field is free-form and any value is at home in a text input.
	const offeredValues = $derived.by<string[] | null>(() => {
		if (resourceOptions.length > 0) return resourceOptions.map((option) => option.id);
		if (picksFromChoices) return fieldDef?.values ?? [];
		if (isRelation) return schemaStore.items;
		return null;
	});

	// Values present in the row that the widget's options don't list — a
	// departed person, a broken link, a hand-edited clause. A plain
	// select cannot show them: they render invisible, and the next edit
	// silently drops them from the filter. Same treatment as the item
	// editor: they join the options, marked, until someone picks
	// something else.
	const strayValues = $derived.by(() => {
		if (offeredValues === null) return [];
		const known = new Set(offeredValues);
		const present = isMulti ? selectedValues : scalarValue === '' ? [] : [scalarValue];
		return present.filter((entry) => !known.has(entry));
	});

	function withCurrentOperator(offered: Operator[], current: Operator | ''): Operator[] {
		if (current === '' || offered.includes(current)) return offered;
		return [...offered, current];
	}

	function chooseField(name: string): void {
		const nextType = schemaStore.field(name)?.field_type;
		const nextOperators = nextType ? schemaStore.operatorsFor(nextType) : [];
		// Drop the operator and its operand if the operator no longer applies.
		const keep = row.operator !== '' && nextOperators.includes(row.operator);
		if (keep) {
			onchange({ ...row, field: name });
			return;
		}
		onchange({ ...row, field: name, operator: '', value: null, values: [] });
	}

	function chooseOperator(next: Operator | ''): void {
		// Converts the operand between the scalar and list forms rather than
		// carrying one across where it doesn't belong.
		onchange(withOperator(row, next));
	}

	// Typed handler so ESLint sees `currentTarget` as an element, not `any`
	// (the operator cast on an inline arrow trips no-unsafe-member-access).
	function onOperatorChange(event: Event & { currentTarget: HTMLSelectElement }): void {
		chooseOperator(event.currentTarget.value as Operator | '');
	}

	function setValue(value: string): void {
		onchange({ ...row, value });
	}

	function toggleValue(option: string, checked: boolean): void {
		const set = new Set(selectedValues);
		if (checked) set.add(option);
		else set.delete(option);
		setValues([...set]);
	}

	// Members stay members — the comma-join now happens only in the Rust
	// serializer, on the way out to the clause string.
	function setValues(values: string[]): void {
		onchange({ ...row, values });
	}

	function onMultiSelectChange(event: Event & { currentTarget: HTMLSelectElement }): void {
		setValues(Array.from(event.currentTarget.selectedOptions, (option) => option.value));
	}

	function updateMember(memberIndex: number, memberValue: string): void {
		const values = [...selectedValues];
		values[memberIndex] = memberValue;
		setValues(values);
	}

	function removeMember(memberIndex: number): void {
		setValues(selectedValues.filter((_, index) => index !== memberIndex));
	}

	function addMember(): void {
		setValues([...selectedValues, '']);
	}
</script>

<div class="row">
	<select
		class="field"
		value={row.field}
		onchange={(event) => {
			chooseField(event.currentTarget.value);
		}}
	>
		<option value="" disabled>Field…</option>
		{#each schemaStore.fields as field (field.name)}
			<option value={field.name}>{field.name}</option>
		{/each}
	</select>

	<select
		class="operator"
		value={row.operator}
		disabled={row.field === ''}
		onchange={onOperatorChange}
	>
		<option value="" disabled>is…</option>
		{#each operators as operator (operator)}
			<option value={operator}>{operatorLabel(operator)}</option>
		{/each}
	</select>

	{#if showValue}
		<div class="value">
			{#if isMulti && picksFromChoices}
				<!-- `is any of` / `is none of`: pick several known values. -->
				<div class="checks">
					{#each fieldDef?.values ?? [] as option (option)}
						<label class="check">
							<input
								type="checkbox"
								checked={selectedValues.includes(option)}
								onchange={(event) => {
									toggleValue(option, event.currentTarget.checked);
								}}
							/>
							{option}
						</label>
					{/each}
					{#each strayValues as stray (stray)}
						<label class="check">
							<input
								type="checkbox"
								checked
								onchange={(event) => {
									toggleValue(stray, event.currentTarget.checked);
								}}
							/>
							{stray} (unknown)
						</label>
					{/each}
				</div>
			{:else if isMulti && resourceOptions.length > 0}
				<!-- `is any of` over a resource: pick several entries by label. -->
				<select multiple size="5" onchange={onMultiSelectChange}>
					{#each resourceOptions as option (option.id)}
						<option value={option.id} selected={selectedValues.includes(option.id)}
							>{option.label}</option
						>
					{/each}
					{#each strayValues as stray (stray)}
						<option value={stray} selected>{stray} (unknown)</option>
					{/each}
				</select>
			{:else if isMulti && isRelation}
				<!-- Relations: too many ids for checkboxes, so a list box. -->
				<select multiple size="5" onchange={onMultiSelectChange}>
					{#each schemaStore.items as id (id)}
						<option value={id} selected={selectedValues.includes(id)}>{prettifyId(id)}</option>
					{/each}
					{#each strayValues as stray (stray)}
						<option value={stray} selected>{stray} (unknown)</option>
					{/each}
				</select>
			{:else if isMulti}
				<!-- Free-form members — `in` on a string or list field has no
				     option set to pick from, so members are typed, one input
				     each. Members stay members; the comma-join (and its
				     comma rejection) lives in the Rust serializer. -->
				<div class="members">
					{#each selectedValues as member, memberIndex (memberIndex)}
						<div class="member">
							<input
								type="text"
								value={member}
								onchange={(event) => {
									updateMember(memberIndex, event.currentTarget.value);
								}}
							/>
							<button
								type="button"
								class="remove"
								aria-label="Remove value"
								onclick={() => {
									removeMember(memberIndex);
								}}>×</button
							>
						</div>
					{/each}
					<button type="button" class="add" onclick={addMember}>+ value</button>
				</div>
			{:else if resourceOptions.length > 0}
				<select
					value={scalarValue}
					onchange={(event) => {
						setValue(event.currentTarget.value);
					}}
				>
					<option value="" disabled>Value…</option>
					{#each resourceOptions as option (option.id)}
						<option value={option.id}>{option.label}</option>
					{/each}
					{#each strayValues as stray (stray)}
						<option value={stray}>{stray} (unknown)</option>
					{/each}
				</select>
			{:else if fieldType === 'choice' || fieldType === 'multichoice'}
				<select
					value={scalarValue}
					onchange={(event) => {
						setValue(event.currentTarget.value);
					}}
				>
					<option value="" disabled>Value…</option>
					{#each fieldDef?.values ?? [] as option (option)}
						<option value={option}>{option}</option>
					{/each}
					{#each strayValues as stray (stray)}
						<option value={stray}>{stray} (unknown)</option>
					{/each}
				</select>
			{:else if fieldType === 'boolean'}
				<select
					value={scalarValue}
					onchange={(event) => {
						setValue(event.currentTarget.value);
					}}
				>
					<option value="" disabled>Value…</option>
					<option value="true">true</option>
					<option value="false">false</option>
				</select>
			{:else if fieldType === 'date'}
				<input
					type="date"
					value={scalarValue}
					onchange={(event) => {
						setValue(event.currentTarget.value);
					}}
				/>
			{:else if fieldType === 'integer' || fieldType === 'float'}
				<input
					type="number"
					step={fieldType === 'integer' ? '1' : 'any'}
					value={scalarValue}
					onchange={(event) => {
						setValue(event.currentTarget.value);
					}}
				/>
			{:else if fieldType === 'link' || fieldType === 'links'}
				<select
					value={scalarValue}
					onchange={(event) => {
						setValue(event.currentTarget.value);
					}}
				>
					<option value="" disabled>Value…</option>
					{#each schemaStore.items as id (id)}
						<option value={id}>{prettifyId(id)}</option>
					{/each}
					{#each strayValues as stray (stray)}
						<option value={stray}>{stray} (unknown)</option>
					{/each}
				</select>
			{:else}
				<!-- string and duration: free text. A resource-backed field
				     lands here too when its list is missing or empty. -->
				<input
					type="text"
					value={scalarValue}
					onchange={(event) => {
						setValue(event.currentTarget.value);
					}}
				/>
			{/if}
		</div>
	{/if}

	<button type="button" class="remove" aria-label="Remove condition" onclick={onremove}>×</button>
</div>

<style>
	.row {
		display: flex;
		align-items: flex-start;
		gap: var(--space-2);
	}

	select,
	select[multiple],
	input[type='text'],
	input[type='number'],
	input[type='date'] {
		padding: 0.25rem var(--space-2);
		background-color: var(--color-bg);
		color: var(--color-fg);
		border: 1px solid var(--color-border);
		border-radius: var(--radius-sm);
		font-size: var(--text-sm);
	}

	.field {
		min-width: 8rem;
	}

	.operator {
		min-width: 8rem;
	}

	.value {
		flex: 1;
		min-width: 8rem;
	}

	.value select,
	.value input {
		width: 100%;
	}

	.checks {
		display: flex;
		flex-wrap: wrap;
		gap: var(--space-2);
		padding: 0.25rem 0;
	}

	.check {
		display: inline-flex;
		align-items: center;
		gap: 0.25rem;
		font-size: var(--text-sm);
	}

	.members {
		display: flex;
		flex-direction: column;
		gap: var(--space-2);
	}

	.member {
		display: flex;
		align-items: center;
		gap: var(--space-2);
	}

	.member input {
		flex: 1;
	}

	.add {
		align-self: flex-start;
		background: none;
		border: 1px dashed var(--color-border);
		border-radius: var(--radius-sm);
		color: var(--color-fg-muted);
		cursor: pointer;
		font-size: var(--text-sm);
		padding: 0.125rem var(--space-2);
	}

	.remove {
		background: none;
		border: none;
		color: var(--color-fg-muted);
		cursor: pointer;
		font-size: var(--text-lg);
		line-height: 1.5;
		padding: 0 0.25rem;
	}

	.remove:hover {
		color: var(--color-error-fg);
	}
</style>
