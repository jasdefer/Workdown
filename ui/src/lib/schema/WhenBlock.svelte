<!--
  The branch list of the field editor's "Value" section, shown once
  "Conditional" is chosen: one row per branch, in order, because first
  match wins — Up and Down beside Remove, and Add branch under the list
  (`schema-derived-field-editor`, decision 5).

  A branch's condition is a row — left operand, operator, right operand
  — where an operand is a field, a constant, `$today` or a value
  entered through the shared value editor, built for the other side's
  type so a choice offers its values (decision 1). A condition the row
  cannot hold is text, as the server served it (decision 2); a row can
  be turned into a text line and back, each starting empty, since the
  browser does not spell the grammar. The `then` cell is the value
  editor with the field's own spec. The fallback is the field's default
  control, unlocked above as "when no branch holds".

  Owns no state: the draft's recipe is the value, and every change goes
  up as the whole new recipe.
-->
<script lang="ts">
	import type { ComparisonOperator } from '$lib/api/generated/ComparisonOperator';
	import type { ComparisonOperatorData } from '$lib/api/generated/ComparisonOperatorData';
	import type { ConditionShape } from '$lib/api/generated/ConditionShape';
	import type { ConstantData } from '$lib/api/generated/ConstantData';
	import type { FieldDefinitionData } from '$lib/api/generated/FieldDefinitionData';
	import type { FieldValue } from '$lib/api/generated/FieldValue';
	import type { Operand } from '$lib/api/generated/Operand';
	import type { PaletteColor } from '$lib/api/generated/PaletteColor';
	import ValueEditor from '$lib/values/ValueEditor.svelte';
	import type { ValueSpec } from '$lib/values/valueSpec';
	import {
		choicesFacing,
		literalOf,
		literalSpecFor,
		newBranch,
		operandKey,
		thenOf,
		valueOfLiteral,
		withBranch,
		withBranchAdded,
		withBranchMoved,
		withBranchRemoved,
		withOperand,
		type OperandChoice,
		type RowCondition,
		type WhenRecipe
	} from './fieldDraft';

	interface Props {
		value: WhenRecipe;
		/** What a `then` may be: the field's own type and settings. */
		spec: ValueSpec;
		/** What an operand may be, in the order shown. */
		choices: OperandChoice[];
		/** The comparison operators with the symbols the grammar spells them as. */
		operators: ComparisonOperatorData[];
		fields: FieldDefinitionData[];
		constants: ConstantData[];
		items: string[];
		palette: PaletteColor[];
		/** The server's objection to the draft, when it has one. */
		problem: string | null;
		disabled?: boolean;
		onchange: (value: WhenRecipe) => void;
	}

	let {
		value,
		spec,
		choices,
		operators,
		fields,
		constants,
		items,
		palette,
		problem,
		disabled = false,
		onchange
	}: Props = $props();

	type Side = 'left' | 'right';

	function conditionOf(index: number): ConditionShape {
		return value.branches[index]?.condition ?? { kind: 'text', source: '' };
	}

	function setCondition(index: number, condition: ConditionShape): void {
		const branch = value.branches[index];
		if (branch === undefined) return;
		onchange(withBranch(value, index, { ...branch, condition }));
	}

	function setThen(index: number, then: FieldValue | null): void {
		const branch = value.branches[index];
		if (branch === undefined) return;
		onchange(withBranch(value, index, { ...branch, then }));
	}

	function pickOperand(index: number, side: Side, key: string): void {
		const condition = conditionOf(index);
		if (condition.kind !== 'row') return;
		const operand = choices.find((choice) => choice.key === key)?.operand;
		if (operand === undefined) return;
		setCondition(index, withOperand(condition, side, operand, choices, fields, constants));
	}

	function setLiteral(index: number, side: Side, next: FieldValue | null): void {
		const condition = conditionOf(index);
		if (condition.kind !== 'row') return;
		const operand: Operand = { kind: 'literal', value: literalOf(next) };
		setCondition(index, { ...condition, [side]: operand });
	}

	function setOperator(index: number, operator: ComparisonOperator): void {
		const condition = conditionOf(index);
		if (condition.kind !== 'row') return;
		setCondition(index, { ...condition, operator });
	}

	function setSource(index: number, source: string): void {
		setCondition(index, { kind: 'text', source });
	}

	/** The other side of a row, for the literal editor's spec and for what this side may be. */
	function otherSide(condition: RowCondition, side: Side): Operand {
		return side === 'left' ? condition.right : condition.left;
	}

	/**
	 * The picker's options for the side facing `other` — "a value" only
	 * where the grammar has a literal of the other side's type — plus
	 * the operand the file names when the picker cannot offer it.
	 */
	function optionsFor(operand: Operand, other: Operand): OperandChoice[] {
		const offered = choicesFacing(choices, other, fields, constants);
		const key = operandKey(operand);
		if (offered.some((choice) => choice.key === key)) return offered;
		const label =
			operand.kind === 'literal'
				? 'a value (does not fit)'
				: `${key.replace(/^[a-z]+:/, '')} (unknown)`;
		return [...offered, { key, label, operand }];
	}
</script>

<div class="block">
	<ol class="branches">
		{#each value.branches as branch, index (index)}
			{@const condition = branch.condition}
			<li class="branch">
				<div class="condition">
					<span class="word">if</span>
					{#if condition.kind === 'row'}
						{#each ['left', 'right'] as const as side, position (side)}
							{#if position === 1}
								<select
									class="operator"
									aria-label="Operator"
									value={condition.operator}
									{disabled}
									onchange={(event: Event & { currentTarget: HTMLSelectElement }) => {
										setOperator(index, event.currentTarget.value as ComparisonOperator);
									}}
								>
									{#each operators as entry (entry.operator)}
										<option value={entry.operator}>{entry.symbol}</option>
									{/each}
								</select>
							{/if}
							{@const operand = condition[side]}
							<select
								class="operand"
								aria-label="{side === 'left' ? 'Left' : 'Right'} operand"
								value={operandKey(operand)}
								{disabled}
								onchange={(event) => {
									pickOperand(index, side, event.currentTarget.value);
								}}
							>
								{#each optionsFor(operand, otherSide(condition, side)) as choice (choice.key)}
									<option value={choice.key}>{choice.label}</option>
								{/each}
							</select>
							{#if operand.kind === 'literal'}
								<div class="literal">
									<ValueEditor
										spec={literalSpecFor(otherSide(condition, side), fields, constants)}
										value={valueOfLiteral(operand.value)}
										{items}
										{palette}
										{disabled}
										onchange={(next: FieldValue | null) => {
											setLiteral(index, side, next);
										}}
									/>
								</div>
							{/if}
						{/each}
						<button
							type="button"
							class="switch"
							{disabled}
							title="Write the condition as text, for what the row cannot hold"
							onclick={() => {
								setSource(index, '');
							}}>as text</button
						>
					{:else}
						<input
							type="text"
							class="source"
							aria-label="Condition"
							value={condition.source}
							placeholder={'status == "done"'}
							spellcheck="false"
							autocomplete="off"
							{disabled}
							oninput={(event) => {
								setSource(index, event.currentTarget.value);
							}}
						/>
						<button
							type="button"
							class="switch"
							{disabled}
							title="Edit the condition as a row: a field, an operator, a value"
							onclick={() => {
								setCondition(index, newBranch(choices).condition);
							}}>as a row</button
						>
					{/if}
				</div>
				<div class="then">
					<span class="word">then</span>
					<div class="then-editor">
						<ValueEditor
							{spec}
							value={thenOf(branch)}
							{items}
							{palette}
							{disabled}
							onchange={(next: FieldValue | null) => {
								setThen(index, next);
							}}
						/>
					</div>
					<button
						type="button"
						class="move"
						aria-label="Move branch {index + 1} up"
						title="Move up"
						disabled={disabled || index === 0}
						onclick={() => {
							onchange(withBranchMoved(value, index, 'up'));
						}}>↑</button
					>
					<button
						type="button"
						class="move"
						aria-label="Move branch {index + 1} down"
						title="Move down"
						disabled={disabled || index === value.branches.length - 1}
						onclick={() => {
							onchange(withBranchMoved(value, index, 'down'));
						}}>↓</button
					>
					<button
						type="button"
						class="remove"
						aria-label="Remove branch {index + 1}"
						title="Remove"
						disabled={disabled || value.branches.length === 1}
						onclick={() => {
							onchange(withBranchRemoved(value, index));
						}}>×</button
					>
				</div>
			</li>
		{/each}
	</ol>
	<button
		type="button"
		class="add"
		{disabled}
		onclick={() => {
			onchange(withBranchAdded(value, newBranch(choices)));
		}}>Add branch</button
	>
	<!-- Below the buttons, not above them: the verdict goes blank the
	     moment the draft changes, and a `then` typed into a box is
	     committed by the blur that pressing a button causes. Above the
	     list, that collapse moved every button under the pointer between
	     mouse-down and mouse-up and the click was lost. -->
	{#if problem !== null}
		<span class="hint warning" role="alert">{problem}</span>
	{/if}
	<span class="hint">
		Branches are tried top to bottom and the first whose condition holds supplies the value; one
		whose condition cannot be answered on an item is skipped. The default above is the fallback when
		none holds. A value written by hand wins.
	</span>
</div>

<style>
	.block {
		display: flex;
		flex-direction: column;
		gap: var(--space-2);
	}

	.branches {
		display: flex;
		flex-direction: column;
		gap: var(--space-2);
		margin: 0;
		padding: 0;
		list-style: none;
	}

	.branch {
		display: flex;
		flex-direction: column;
		gap: var(--space-1);
		padding: var(--space-2);
		border: 1px solid var(--color-border);
		border-radius: var(--radius-sm);
	}

	.condition,
	.then {
		display: flex;
		flex-wrap: wrap;
		align-items: center;
		gap: var(--space-2);
	}

	.word {
		width: 2.5rem;
		font-size: var(--text-sm);
		font-weight: 600;
		color: var(--color-fg-muted);
	}

	.literal,
	.then-editor {
		flex: 1 1 8rem;
		min-width: 6rem;
	}

	.operand,
	.operator,
	.source {
		padding: 0.25rem var(--space-2);
		background-color: var(--color-bg);
		color: var(--color-fg);
		border: 1px solid var(--color-border);
		border-radius: var(--radius-sm);
		font-size: var(--text-sm);
		font-family: inherit;
	}

	.operator {
		font-family: var(--font-mono);
	}

	.source {
		flex: 1 1 12rem;
		font-family: var(--font-mono);
	}

	.switch {
		background: none;
		border: none;
		padding: 0;
		color: var(--color-fg-muted);
		font-size: var(--text-sm);
		text-decoration: underline;
		cursor: pointer;
	}

	.move,
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

	.move,
	.remove {
		width: 1.75rem;
		padding-left: 0;
		padding-right: 0;
	}

	.add {
		align-self: flex-start;
	}

	.remove:not(:disabled):hover {
		color: var(--color-error-fg);
	}

	.move:disabled,
	.remove:disabled,
	.add:disabled,
	.switch:disabled {
		opacity: 0.5;
		cursor: default;
	}

	.hint {
		font-size: var(--text-sm);
		color: var(--color-fg-muted);
	}

	.hint.warning {
		color: var(--color-warning-fg);
	}
</style>
