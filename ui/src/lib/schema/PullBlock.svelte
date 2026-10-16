<!--
  The pull pickers of the field editor's "Value" section, shown once
  "Pulled from linked items" is chosen: through which relation, which
  field is read on the items it points at, and how the collected values
  are combined, plus the `error_on_missing` box
  (`schema-derived-field-editor`, decision 6).

  The pickers offer only what the load accepts: the `link` and `links`
  fields that forbid cycles (`pullRelations`), the fields whose type the
  server's table lists as a source for this field's type, and for the
  chosen field the functions that reduce it into this type
  (`pullSources`). A choice the file names but the pickers cannot offer
  stays in the options, marked, so the panel shows what the file says;
  the preview reports the problem and Save stays grey.

  Owns no state: the draft's pull is the value, and a change goes up
  as the whole new config.
-->
<script lang="ts">
	import type { AggregateFunction } from '$lib/api/generated/AggregateFunction';
	import type { PullConfig } from '$lib/api/generated/PullConfig';
	import { withPullSource, type PullSourceField } from './fieldDraft';

	interface Props {
		value: PullConfig;
		/** The `link` and `links` fields that forbid cycles, in declaration order. */
		relations: string[];
		/** The fields a pull into this type may read, each with its functions. */
		sources: PullSourceField[];
		disabled?: boolean;
		onchange: (value: PullConfig) => void;
	}

	let { value, relations, sources, disabled = false, onchange }: Props = $props();

	const strayRelation = $derived(relations.includes(value.over) ? null : value.over);
	const source = $derived(sources.find((entry) => entry.name === value.field) ?? null);
	const functions = $derived(source?.functions ?? []);
	const strayFunction = $derived(functions.includes(value.function) ? null : value.function);

	function onRelationChange(event: Event & { currentTarget: HTMLSelectElement }): void {
		onchange({ ...value, over: event.currentTarget.value });
	}

	function onSourceChange(event: Event & { currentTarget: HTMLSelectElement }): void {
		onchange(withPullSource(value, sources, event.currentTarget.value));
	}

	function onFunctionChange(event: Event & { currentTarget: HTMLSelectElement }): void {
		onchange({ ...value, function: event.currentTarget.value as AggregateFunction });
	}

	function onMissingChange(event: Event & { currentTarget: HTMLInputElement }): void {
		onchange({ ...value, error_on_missing: event.currentTarget.checked });
	}
</script>

<div class="block">
	<div class="pickers">
		<label class="row">
			<span class="label">Through</span>
			<select value={value.over} {disabled} onchange={onRelationChange}>
				{#each relations as name (name)}
					<option value={name}>{name}</option>
				{/each}
				{#if strayRelation !== null}
					<option value={strayRelation}>{strayRelation} (not followable)</option>
				{/if}
			</select>
		</label>
		<label class="row">
			<span class="label">Read</span>
			<select value={value.field} {disabled} onchange={onSourceChange}>
				{#each sources as entry (entry.name)}
					<option value={entry.name}>{entry.name}</option>
				{/each}
				{#if source === null}
					<option value={value.field}>{value.field} (cannot be read)</option>
				{/if}
			</select>
		</label>
		<label class="row">
			<span class="label">Combine with</span>
			<select value={value.function} {disabled} onchange={onFunctionChange}>
				{#each functions as name (name)}
					<option value={name}>{name}</option>
				{/each}
				{#if strayFunction !== null}
					<option value={strayFunction}>{strayFunction} (does not fit)</option>
				{/if}
			</select>
		</label>
	</div>
	{#if strayRelation !== null}
		<span class="hint warning">
			<code>{strayRelation}</code> is not a <code>link</code> or <code>links</code> field that forbids
			cycles.
		</span>
	{:else if source === null}
		<span class="hint warning">
			No function reduces <code>{value.field}</code> into this field's type.
		</span>
	{:else if strayFunction !== null}
		<span class="hint warning">
			<code>{strayFunction}</code> of <code>{value.field}</code> does not produce this field's type.
		</span>
	{:else}
		<span class="hint">
			An item's value is the <code>{value.function}</code> of <code>{value.field}</code> on the
			items its <code>{value.over}</code> points at; a value written by hand wins.
		</span>
	{/if}
	<label class="row inline">
		<input type="checkbox" checked={value.error_on_missing} {disabled} onchange={onMissingChange} />
		<span class="label">Report linked items without a value</span>
	</label>
	<span class="hint">
		Either way one linked item without <code>{value.field}</code> means no value here, never a partial
		result. Unchecked, that is silent; checked, every such item is reported as an error.
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

	.pickers {
		display: grid;
		grid-template-columns: 1fr 1fr 1fr;
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
