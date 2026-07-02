<!--
  A metric view's repeatable rows: label? + aggregate + value?. Owns the
  draft rows (keyed by a local id, the same idiom as the filter builder)
  and reports the definition-shaped `metrics:` array up on every edit —
  including once on mount, so a freshly chosen metric kind starts with
  one complete count row.
-->
<script lang="ts">
	import { onMount } from 'svelte';
	import { schemaStore } from '$lib/stores/schema.svelte';
	import { AGGREGATES, fieldFits } from './viewKinds';

	interface Props {
		/** Fires with the `metrics:` slot value whenever the rows change. */
		onchange: (metrics: Record<string, unknown>[]) => void;
	}

	let { onchange }: Props = $props();

	interface RowDraft {
		/** Stable key for `{#each}`; never leaves the component. */
		localId: number;
		label: string;
		aggregate: string;
		value: string;
	}

	let idCounter = 0;
	const nextId = (): number => (idCounter += 1);
	const newRow = (): RowDraft => ({ localId: nextId(), label: '', aggregate: 'count', value: '' });

	let rows = $state<RowDraft[]>([newRow()]);

	/** A row in the shape one entry of the `metrics:` slot takes. */
	function toEntry(row: RowDraft): Record<string, unknown> {
		const entry: Record<string, unknown> = { aggregate: row.aggregate };
		if (row.label.trim() !== '') entry.label = row.label.trim();
		if (row.value !== '') entry.value = row.value;
		return entry;
	}

	function emit(): void {
		onchange(rows.map(toEntry));
	}

	onMount(emit);

	function update(localId: number, patch: Partial<RowDraft>): void {
		rows = rows.map((row) => (row.localId === localId ? { ...row, ...patch } : row));
		emit();
	}

	function add(): void {
		rows = [...rows, newRow()];
		emit();
	}

	function remove(localId: number): void {
		rows = rows.filter((row) => row.localId !== localId);
		emit();
	}

	function valueFieldOptions() {
		return schemaStore.fields.filter((field) =>
			fieldFits(field.field_type, ['integer', 'float', 'duration', 'date'])
		);
	}
</script>

<div class="metrics">
	{#each rows as row (row.localId)}
		<div class="metric-row">
			<input
				type="text"
				placeholder="label (optional)"
				value={row.label}
				onchange={(event) => {
					update(row.localId, { label: event.currentTarget.value });
				}}
			/>
			<select
				value={row.aggregate}
				onchange={(event) => {
					update(row.localId, { aggregate: event.currentTarget.value });
				}}
			>
				{#each AGGREGATES as aggregate (aggregate)}
					<option value={aggregate}>{aggregate}</option>
				{/each}
			</select>
			<select
				value={row.value}
				onchange={(event) => {
					update(row.localId, { value: event.currentTarget.value });
				}}
			>
				<option value="">— no value —</option>
				{#each valueFieldOptions() as field (field.name)}
					<option value={field.name}>{field.name}</option>
				{/each}
			</select>
			<button
				type="button"
				class="remove"
				aria-label="Remove metric"
				onclick={() => {
					remove(row.localId);
				}}>×</button
			>
		</div>
	{/each}
	<button type="button" class="ghost" onclick={add}>+ Add metric</button>
</div>

<style>
	.metrics {
		display: flex;
		flex-direction: column;
		gap: var(--space-2);
	}

	.metric-row {
		display: flex;
		gap: var(--space-2);
		align-items: center;
	}

	input[type='text'],
	select {
		padding: 0.25rem var(--space-2);
		background-color: var(--color-bg);
		color: var(--color-fg);
		border: 1px solid var(--color-border);
		border-radius: var(--radius-sm);
		font-size: var(--text-sm);
	}

	.metric-row input,
	.metric-row select {
		flex: 1;
		min-width: 0;
	}

	.ghost {
		align-self: flex-start;
		background: none;
		border: 1px solid var(--color-border);
		border-radius: var(--radius-sm);
		color: var(--color-fg-muted);
		padding: 0.25rem var(--space-2);
		font-size: var(--text-sm);
		cursor: pointer;
	}

	.remove {
		background: none;
		border: none;
		color: var(--color-fg-muted);
		cursor: pointer;
		font-size: var(--text-lg);
		line-height: 1;
		padding: 0 0.25rem;
	}

	.remove:hover {
		color: var(--color-error-fg);
	}
</style>
