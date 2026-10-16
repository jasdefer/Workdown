<!--
  The type-specific block for `choice` and `multichoice`: the ordered
  list of allowed values. Order is what boards and dropdowns show, so
  each row has Up and Down beside Remove; a value is renamed in its own
  input and a new one is typed into the input under the list and added
  with Enter or the button. Blank text and a value already listed are
  refused in place, with the reason, so a save is never refused for
  something the block let in; the server holds a hand-written list to
  the same rule.

  Renaming or removing a value does not touch the items holding it;
  they keep the old text and get a warning after the save, which the
  line under the list says. Nothing else is automatic: a default that
  names a removed or renamed value is left as it is, and the save comes
  back with the server's refusal.

  Owns no draft state: the draft's shape is the value, and every change
  goes up through `onchange` as the whole new shape. What it keeps for
  itself is the text being typed for a new value and the last refusal,
  both gone once a change is accepted. The host decides what a problem
  with the list means (an empty list greys out Save) and passes the
  message down to be shown here.
-->
<script lang="ts">
	import {
		valueProblem,
		withValueAdded,
		withValueMoved,
		withValueRemoved,
		withValueRenamed,
		type ValuesShape
	} from './fieldDraft';

	interface Props {
		shape: ValuesShape;
		/** Why the list cannot be saved as it stands; `null` when it can. */
		problem: string | null;
		disabled?: boolean;
		onchange: (shape: ValuesShape) => void;
	}

	let { shape, problem, disabled = false, onchange }: Props = $props();

	/** The text typed for a new value, not yet in the list. */
	let pending = $state('');
	/** Why the last add or rename was refused; cleared by the next accepted change. */
	let refusal = $state<string | null>(null);

	function accept(next: ValuesShape): void {
		refusal = null;
		onchange(next);
	}

	function add(): void {
		const issue = valueProblem(shape, pending);
		if (issue !== null) {
			refusal = issue;
			return;
		}
		accept(withValueAdded(shape, pending));
		pending = '';
	}

	function onPendingKeydown(event: KeyboardEvent): void {
		if (event.key !== 'Enter') return;
		// Enter in this input adds a value; it must not submit the form
		// around the panel, which would save the field.
		event.preventDefault();
		add();
	}

	function rename(index: number, event: Event & { currentTarget: HTMLInputElement }): void {
		const text = event.currentTarget.value;
		if (text.trim() === shape.values[index]) return;
		const issue = valueProblem(shape, text, index);
		if (issue !== null) {
			refusal = issue;
			// Put the list's value back: the draft did not change, and
			// an input left showing refused text would look saved.
			event.currentTarget.value = shape.values[index] ?? '';
			return;
		}
		accept(withValueRenamed(shape, index, text));
	}

	const shown = $derived(problem ?? refusal);
</script>

<fieldset class="values" aria-describedby={shown === null ? undefined : 'values-problem'}>
	<legend class="label">Values</legend>
	{#if shape.values.length > 0}
		<ol class="list">
			{#each shape.values as value, index (index)}
				<li class="row">
					<input
						type="text"
						class="value"
						{value}
						aria-label="Value {index + 1}"
						spellcheck="false"
						{disabled}
						onchange={(event) => {
							rename(index, event);
						}}
					/>
					<button
						type="button"
						class="move"
						aria-label="Move '{value}' up"
						title="Move up"
						disabled={disabled || index === 0}
						onclick={() => {
							accept(withValueMoved(shape, index, 'up'));
						}}>↑</button
					>
					<button
						type="button"
						class="move"
						aria-label="Move '{value}' down"
						title="Move down"
						disabled={disabled || index === shape.values.length - 1}
						onclick={() => {
							accept(withValueMoved(shape, index, 'down'));
						}}>↓</button
					>
					<button
						type="button"
						class="remove"
						aria-label="Remove '{value}'"
						title="Remove"
						{disabled}
						onclick={() => {
							accept(withValueRemoved(shape, index));
						}}>×</button
					>
				</li>
			{/each}
		</ol>
	{/if}
	<div class="add">
		<input
			type="text"
			class="value"
			bind:value={pending}
			placeholder="new value"
			aria-label="New value"
			spellcheck="false"
			{disabled}
			onkeydown={onPendingKeydown}
		/>
		<button type="button" class="add-button" {disabled} onclick={add}>Add</button>
	</div>
	{#if shown !== null}
		<span id="values-problem" class="hint warning" role="alert">{shown}</span>
	{:else}
		<span class="hint">
			Renaming or removing a value does not change the items holding it; they get a warning after
			saving.
		</span>
	{/if}
</fieldset>

<style>
	.values {
		display: flex;
		flex-direction: column;
		gap: var(--space-1);
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

	.list {
		display: flex;
		flex-direction: column;
		gap: var(--space-1);
		margin: 0;
		padding: 0;
		list-style: none;
	}

	.row,
	.add {
		display: flex;
		align-items: center;
		gap: var(--space-1);
	}

	.value {
		flex: 1;
		min-width: 0;
		padding: 0.25rem var(--space-2);
		background-color: var(--color-bg);
		color: var(--color-fg);
		border: 1px solid var(--color-border);
		border-radius: var(--radius-sm);
		font-size: var(--text-sm);
		font-family: var(--font-mono);
	}

	.move,
	.remove,
	.add-button {
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

	.remove:not(:disabled):hover {
		color: var(--color-error-fg);
	}

	.move:disabled,
	.remove:disabled,
	.add-button:disabled {
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
