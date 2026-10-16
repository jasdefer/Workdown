<!--
  The formula editor of the field editor's "Value" section, shown once
  "Computed" is chosen: the expression as text, made friendly rather
  than built from rows (`schema-derived-field-editor`, decision 1). The
  things a formula may name — the readable fields, the constants,
  `$today` — are listed under the box and a click inserts one at the
  caret; the server's verdict on the draft is shown under the box too,
  with a caret under the column it names, so a wrong name or an
  operation the type does not allow is pointed at, not just described.
  The grammar stays in Rust: nothing here parses the text.

  The options: `round`, only on a `date` field (nothing else rounds to
  days), and `error_on_missing` (decision 4).

  Owns no state but the input element, needed for the caret: the
  draft's recipe is the value, and a change goes up as the whole new
  recipe.
-->
<script lang="ts">
	import { tick } from 'svelte';
	import type { FieldType } from '$lib/api/generated/FieldType';
	import type { RoundMode } from '$lib/api/generated/RoundMode';
	import { insertAt, type ComputeRecipe, type ExpressionReference } from './fieldDraft';

	interface Props {
		value: ComputeRecipe;
		fieldType: FieldType;
		/** What the formula may name, in the order shown. */
		references: ExpressionReference[];
		/** The server's objection to the draft, when it has one. */
		problem: string | null;
		/** The one-based column the objection points at, when it does. */
		problemColumn: number | null;
		disabled?: boolean;
		onchange: (value: ComputeRecipe) => void;
	}

	let {
		value,
		fieldType,
		references,
		problem,
		problemColumn,
		disabled = false,
		onchange
	}: Props = $props();

	let input = $state<HTMLInputElement>();

	// A caret line under the input, in the same monospace, so the mark
	// lands under the character the column names.
	const marker = $derived(
		problem !== null && problemColumn !== null && problemColumn >= 1
			? ' '.repeat(problemColumn - 1) + '^'
			: null
	);

	const roundModes: { mode: RoundMode; label: string }[] = [
		{ mode: 'nearest', label: 'Nearest day' },
		{ mode: 'floor', label: 'Last full day (floor)' },
		{ mode: 'ceil', label: 'The day it spills into (ceil)' }
	];

	function onExpressionInput(event: Event & { currentTarget: HTMLInputElement }): void {
		onchange({ ...value, expression: event.currentTarget.value });
	}

	async function insert(text: string): Promise<void> {
		const start = input?.selectionStart ?? value.expression.length;
		const end = input?.selectionEnd ?? start;
		const next = insertAt(value.expression, start, end, text);
		onchange({ ...value, expression: next.expression });
		await tick();
		input?.focus();
		input?.setSelectionRange(next.caret, next.caret);
	}

	function onRoundChange(event: Event & { currentTarget: HTMLSelectElement }): void {
		onchange({ ...value, round: event.currentTarget.value as RoundMode });
	}

	function onMissingChange(event: Event & { currentTarget: HTMLInputElement }): void {
		onchange({ ...value, error_on_missing: event.currentTarget.checked });
	}
</script>

<div class="block">
	<label class="row">
		<span class="label">Formula</span>
		<input
			type="text"
			class="expression"
			bind:this={input}
			value={value.expression}
			{disabled}
			spellcheck="false"
			autocomplete="off"
			placeholder="start_date + duration"
			oninput={onExpressionInput}
		/>
	</label>
	{#if marker !== null}
		<pre class="marker" aria-hidden="true">{marker}</pre>
	{/if}
	{#if problem !== null}
		<span class="hint warning" role="alert">{problem}</span>
	{/if}
	<div class="references" aria-label="Insert">
		<span class="hint">Insert:</span>
		{#each references as reference (reference.text)}
			<button
				type="button"
				class="chip"
				{disabled}
				title={reference.detail}
				onclick={() => void insert(reference.text)}
			>
				{reference.text}
			</button>
		{/each}
	</div>
	<span class="hint">
		Arithmetic with <code>+ - * /</code>, comparisons, brackets, numbers and quoted text; the result
		must be a {fieldType}. The value is worked out when the project loads and never written to an
		item; a value written by hand wins.
	</span>
	{#if fieldType === 'date'}
		<label class="row">
			<span class="label">Round</span>
			<select value={value.round} {disabled} onchange={onRoundChange}>
				{#each roundModes as entry (entry.mode)}
					<option value={entry.mode}>{entry.label}</option>
				{/each}
			</select>
			<span class="hint">How a result with part of a day left over lands on a calendar day.</span>
		</label>
	{/if}
	<label class="row inline">
		<input type="checkbox" checked={value.error_on_missing} {disabled} onchange={onMissingChange} />
		<span class="label">Report items missing an input</span>
	</label>
	<span class="hint">
		Unchecked, an item without one of the fields the formula reads simply has no value. Checked,
		every such item is reported as an error.
	</span>
</div>

<style>
	.block {
		display: flex;
		flex-direction: column;
		gap: var(--space-2);
	}

	.row {
		display: flex;
		flex-direction: column;
		gap: var(--space-1);
	}

	.row.inline {
		flex-direction: row;
		align-items: center;
		gap: var(--space-2);
	}

	.label {
		font-size: var(--text-sm);
		font-weight: 600;
		color: var(--color-fg-muted);
	}

	input[type='text'],
	select {
		width: 100%;
		padding: 0.25rem var(--space-2);
		background-color: var(--color-bg);
		color: var(--color-fg);
		border: 1px solid var(--color-border);
		border-radius: var(--radius-sm);
		font-size: var(--text-sm);
		font-family: inherit;
	}

	.expression {
		font-family: var(--font-mono);
	}

	/* The same font and horizontal padding as the input, so column n of
	   the marker sits under column n of the text. */
	.marker {
		margin: calc(-1 * var(--space-2)) 0 0;
		padding: 0 calc(var(--space-2) + 1px);
		font-family: var(--font-mono);
		font-size: var(--text-sm);
		line-height: 1;
		color: var(--color-warning-fg);
		background: none;
		white-space: pre;
		overflow-x: auto;
	}

	.references {
		display: flex;
		flex-wrap: wrap;
		align-items: center;
		gap: var(--space-1);
	}

	.chip {
		padding: 0.1rem var(--space-2);
		background-color: var(--color-surface);
		color: var(--color-fg);
		border: 1px solid var(--color-border);
		border-radius: var(--radius-sm);
		font-family: var(--font-mono);
		font-size: var(--text-sm);
		cursor: pointer;
	}

	.chip:disabled {
		opacity: 0.5;
		cursor: default;
	}

	.hint {
		font-size: var(--text-sm);
		color: var(--color-fg-muted);
	}

	.hint code {
		background-color: transparent;
		padding: 0;
	}

	.hint.warning {
		color: var(--color-warning-fg);
	}
</style>
