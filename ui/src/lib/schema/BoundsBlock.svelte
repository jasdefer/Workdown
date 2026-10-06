<!--
  The type-specific block for `integer`, `float` and `duration`: the
  inclusive `min` and `max` bounds, side by side. Each bound is entered
  with the same `ValueEditor` an item's value of that type uses, so an
  integer bound steps by whole numbers, a float bound takes any number,
  and a duration bound reads `2d`, not `2` — the server writes the text
  as typed and refuses a spelling it cannot parse. Clearing an input
  unsets the bound, which removes the key from the file.

  Owns no state: the draft's shape is the value, and every change goes
  up through `onchange` as the whole new shape. The host decides what a
  problem with the pair means (it greys out Save) and passes the
  message down to be shown here, under the inputs it is about.
-->
<script lang="ts">
	import type { FieldType } from '$lib/api/generated/FieldType';
	import type { FieldValue } from '$lib/api/generated/FieldValue';
	import ValueEditor from '$lib/values/ValueEditor.svelte';
	import type { ValueSpec } from '$lib/values/valueSpec';
	import { withBound, type BoundedShape } from './fieldDraft';

	interface Props {
		shape: BoundedShape;
		fieldType: FieldType;
		/** Why the pair cannot be saved as it stands; `null` when it can. */
		problem: string | null;
		disabled?: boolean;
		onchange: (shape: BoundedShape) => void;
	}

	let { shape, fieldType, problem, disabled = false, onchange }: Props = $props();

	// A bound is a bare value of the field's type: never required (an
	// empty input is "no bound"), and not itself bounded.
	const spec = $derived<ValueSpec>({
		fieldType,
		required: false,
		values: [],
		min: null,
		max: null
	});

	function onMinChange(value: FieldValue | null): void {
		onchange(withBound(shape, 'min', value));
	}

	function onMaxChange(value: FieldValue | null): void {
		onchange(withBound(shape, 'max', value));
	}
</script>

<fieldset class="bounds" aria-describedby={problem === null ? undefined : 'bounds-problem'}>
	<legend class="label">Bounds</legend>
	<div class="pair">
		<label class="bound">
			<span class="bound-label">Min</span>
			<ValueEditor {spec} value={shape.min} items={[]} {disabled} onchange={onMinChange} />
		</label>
		<label class="bound">
			<span class="bound-label">Max</span>
			<ValueEditor {spec} value={shape.max} items={[]} {disabled} onchange={onMaxChange} />
		</label>
	</div>
	{#if problem !== null}
		<span id="bounds-problem" class="hint warning" role="alert">{problem}</span>
	{:else}
		<span class="hint">Inclusive. Leave a bound empty for none.</span>
	{/if}
</fieldset>

<style>
	.bounds {
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

	.pair {
		display: grid;
		grid-template-columns: 1fr 1fr;
		gap: var(--space-3);
	}

	.bound {
		display: flex;
		flex-direction: column;
		gap: 0.15rem;
		min-width: 0;
	}

	.bound-label {
		font-size: var(--text-sm);
		color: var(--color-fg-muted);
	}

	.hint {
		font-size: var(--text-sm);
		color: var(--color-fg-muted);
	}

	.hint.warning {
		color: var(--color-warning-fg);
	}
</style>
