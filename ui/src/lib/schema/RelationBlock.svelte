<!--
  The type-specific block for `link` and `links`: whether items may
  form a cycle through the relation, and the name the relation has
  from the other side (`parent` is read as `children` from the child).

  Cycles are one box, "Forbid cycles", because the file's three
  spellings come down to one choice: only an explicit `false` turns
  the cycle check on (see `forbidsCycles`). The inverse is free text;
  blank means none. Neither is judged here. A cycle the items already
  form shows as the existing warning after the save, and the inverse
  is held to the identifier and collision rules the server keeps for
  field names, with its message coming back to the panel on save.

  Owns no state: the draft's shape is the value, and a change goes up
  as the whole new shape.
-->
<script lang="ts">
	import { forbidsCycles, withForbidCycles, withInverse, type RelationShape } from './fieldDraft';

	interface Props {
		shape: RelationShape;
		disabled?: boolean;
		onchange: (shape: RelationShape) => void;
	}

	let { shape, disabled = false, onchange }: Props = $props();

	function onForbidChange(event: Event & { currentTarget: HTMLInputElement }): void {
		onchange(withForbidCycles(shape, event.currentTarget.checked));
	}

	function onInverseChange(event: Event & { currentTarget: HTMLInputElement }): void {
		onchange(withInverse(shape, event.currentTarget.value));
	}
</script>

<div class="block">
	<label class="row inline">
		<input type="checkbox" checked={forbidsCycles(shape)} {disabled} onchange={onForbidChange} />
		<span class="label">Forbid cycles</span>
	</label>
	<span class="hint">
		Checked, a chain of items that loops back on itself is reported. Required for a relation a
		rollup or pull climbs.
	</span>

	<label class="row">
		<span class="label">Inverse</span>
		<input
			type="text"
			class="inverse"
			value={shape.inverse ?? ''}
			placeholder="e.g. children"
			spellcheck="false"
			{disabled}
			onchange={onInverseChange}
		/>
		<span class="hint">
			The relation as seen from the other side, usable in rules like a field. Lowercase letters,
			digits and underscores; leave empty for none.
		</span>
	</label>
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

	.inverse {
		width: 100%;
		padding: 0.25rem var(--space-2);
		background-color: var(--color-bg);
		color: var(--color-fg);
		border: 1px solid var(--color-border);
		border-radius: var(--radius-sm);
		font-size: var(--text-sm);
		font-family: var(--font-mono);
	}

	.hint {
		font-size: var(--text-sm);
		color: var(--color-fg-muted);
	}
</style>
