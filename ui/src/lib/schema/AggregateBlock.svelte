<!--
  The rollup section of the field editor: whether the field's value on
  an item rolls up from the items below it, and if so with which
  function along which relation. Shown for the types the server's table
  gives aggregate functions; its own section rather than part of the
  shape block, because a rollup combines with any way of filling the
  leaves — by hand, by a formula, by a condition, by a pull
  (`schema-derived-field-editor`, decision 3).

  One box switches it on; the pickers then offer only what the parser
  accepts — the type's functions from the served table, and the `link`
  fields that forbid cycles (`rollupRelations`). A relation the file
  names but the picker no longer has (its cycle policy changed, it was
  removed) stays in the options, marked, so the panel shows what the
  file says; the server refuses the save with its reason either way.
  The last box is `error_on_missing`: whether an item below without a
  value of its own is reported.

  Owns no state: the draft's rollup is the value, and a change goes up
  as the whole new config or `null` for none.
-->
<script lang="ts">
	import type { AggregateConfig } from '$lib/api/generated/AggregateConfig';
	import type { AggregateFunction } from '$lib/api/generated/AggregateFunction';

	interface Props {
		value: AggregateConfig | null;
		/** The functions the field's type may reduce with. */
		functions: AggregateFunction[];
		/** The `link` fields that forbid cycles, in declaration order. */
		relations: string[];
		/** The rollup the box switches on; `null` when there is nothing to pick. */
		starting: AggregateConfig | null;
		disabled?: boolean;
		onchange: (value: AggregateConfig | null) => void;
	}

	let { value, functions, relations, starting, disabled = false, onchange }: Props = $props();

	const strayRelation = $derived(
		value !== null && !relations.includes(value.over) ? value.over : null
	);

	function onToggle(event: Event & { currentTarget: HTMLInputElement }): void {
		onchange(event.currentTarget.checked ? starting : null);
	}

	function onFunctionChange(event: Event & { currentTarget: HTMLSelectElement }): void {
		if (value === null) return;
		onchange({ ...value, function: event.currentTarget.value as AggregateFunction });
	}

	function onRelationChange(event: Event & { currentTarget: HTMLSelectElement }): void {
		if (value === null) return;
		onchange({ ...value, over: event.currentTarget.value });
	}

	function onMissingChange(event: Event & { currentTarget: HTMLInputElement }): void {
		if (value === null) return;
		onchange({ ...value, error_on_missing: event.currentTarget.checked });
	}
</script>

<fieldset class="block" aria-label="Roll up">
	<label class="row inline">
		<input
			type="checkbox"
			checked={value !== null}
			disabled={disabled || (value === null && starting === null)}
			onchange={onToggle}
		/>
		<span class="label">Roll up from the items below</span>
	</label>
	{#if value === null}
		{#if starting === null}
			<span class="hint">
				Needs a <code>link</code> field that forbids cycles to climb; none does yet.
			</span>
		{:else}
			<span class="hint">
				Set by hand on the lowest items; every item above gets the combined value of the items under
				it.
			</span>
		{/if}
	{:else}
		<div class="pickers">
			<label class="row">
				<span class="label">Function</span>
				<select value={value.function} {disabled} onchange={onFunctionChange}>
					{#each functions as name (name)}
						<option value={name}>{name}</option>
					{/each}
				</select>
			</label>
			<label class="row">
				<span class="label">Along</span>
				<select value={value.over} {disabled} onchange={onRelationChange}>
					{#each relations as name (name)}
						<option value={name}>{name}</option>
					{/each}
					{#if strayRelation !== null}
						<option value={strayRelation}>{strayRelation} (not climbable)</option>
					{/if}
				</select>
			</label>
		</div>
		{#if strayRelation !== null}
			<span class="hint warning">
				<code>{strayRelation}</code> is not a <code>link</code> field that forbids cycles.
			</span>
		{:else}
			<span class="hint">
				An item's value is the <code>{value.function}</code> of the values of the items whose
				<code>{value.over}</code> points at it; a value written by hand wins.
			</span>
		{/if}
		<label class="row inline">
			<input
				type="checkbox"
				checked={value.error_on_missing}
				{disabled}
				onchange={onMissingChange}
			/>
			<span class="label">Report lowest items without a value</span>
		</label>
		<span class="hint">
			Unchecked, an item at the bottom with no value simply adds nothing, and the total above it
			looks complete. Checked, every such item is reported as an error, so a rollup over half-filled
			items is visible; an item whose value was written by hand covers the items under it.
		</span>
	{/if}
</fieldset>

<style>
	.block {
		display: flex;
		flex-direction: column;
		gap: var(--space-2);
		margin: 0;
		padding: 0;
		border: none;
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

	.pickers {
		display: grid;
		grid-template-columns: 1fr 1fr;
		gap: var(--space-3);
	}

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
