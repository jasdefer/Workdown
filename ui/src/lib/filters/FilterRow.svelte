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
	const isMulti = $derived(fieldType === 'choice' && isMultiValueOperator(row.operator));
	const selectedValues = $derived(row.value ? row.value.split(',') : []);
	const scalarValue = $derived(row.value ?? '');

	function withCurrentOperator(offered: Operator[], current: Operator | ''): Operator[] {
		if (current === '' || offered.includes(current)) return offered;
		return [...offered, current];
	}

	function chooseField(name: string): void {
		const nextType = schemaStore.field(name)?.field_type;
		const nextOperators = nextType ? schemaStore.operatorsFor(nextType) : [];
		// Drop the operator/value if the current operator no longer applies.
		const keep = row.operator !== '' && nextOperators.includes(row.operator);
		onchange({
			...row,
			field: name,
			operator: keep ? row.operator : '',
			value: keep ? row.value : null
		});
	}

	function chooseOperator(next: Operator | ''): void {
		if (next !== '' && isPresenceOperator(next)) {
			onchange({ ...row, operator: next, value: null });
			return;
		}
		// Entering a value operator from a presence one needs a value slot.
		onchange({ ...row, operator: next, value: row.value ?? '' });
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
		setValue([...set].join(','));
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
			{#if isMulti}
				<!-- choice + `is`: multi-select builds an IN clause. -->
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
				</div>
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
				</select>
			{:else}
				<!-- string, duration, resource-backed: free text. -->
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
