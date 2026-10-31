<!--
  A view's item order as rows of field → direction, with add/remove.
  Works entirely in the structured `SortSpec` shape the wire carries —
  the `sort:` key grammar stays server-side, exactly as the filter
  builder never writes clause syntax. Owns the draft rows (keyed by a
  local id, the same idiom as the metric rows editor) and reports the
  complete key list up on every edit, skipping rows whose field is not
  yet picked, so the view form stays the single source of truth for what
  gets saved. Seeded once from `initial`; the host re-keys the component
  to re-seed.

  Every schema field is offered, plus the virtual `id` — the server
  accepts any field type for a sort key — so the picker needs no type
  filter.
-->
<script lang="ts">
	import { untrack } from 'svelte';
	import type { SortDirection } from '$lib/api/generated/SortDirection';
	import type { SortSpec } from '$lib/api/generated/SortSpec';
	import { schemaStore } from '$lib/stores/schema.svelte';

	interface Props {
		/** Persisted keys to seed from (edit mode); absent or empty means id order. */
		initial?: SortSpec[] | undefined;
		/** Fires with the complete key list whenever an edit changes it. */
		onchange: (keys: SortSpec[]) => void;
	}

	let { initial, onchange }: Props = $props();

	interface RowDraft {
		/** Stable key for `{#each}`; never leaves the component. */
		localId: number;
		/** Empty until a field is picked; such a row contributes no key. */
		field: string;
		direction: SortDirection;
	}

	let idCounter = 0;
	const nextId = (): number => (idCounter += 1);
	const newRow = (): RowDraft => ({ localId: nextId(), field: '', direction: 'asc' });

	/** A persisted key back into draft shape — inverse of `toSpec`. */
	const fromSpec = (spec: SortSpec): RowDraft => ({
		localId: nextId(),
		field: spec.field,
		direction: spec.direction
	});

	// Seeded once from the resolved prop — the host re-keys this component
	// when the edited view changes, so this deliberately doesn't track.
	let rows = $state<RowDraft[]>(untrack(() => (initial ?? []).map(fromSpec)));

	function toSpec(row: RowDraft): SortSpec {
		return { field: row.field, direction: row.direction };
	}

	function emit(): void {
		onchange(rows.filter((row) => row.field !== '').map(toSpec));
	}

	function update(localId: number, patch: Partial<RowDraft>): void {
		rows = rows.map((row) => (row.localId === localId ? { ...row, ...patch } : row));
		emit();
	}

	function add(): void {
		rows = [...rows, newRow()];
		// An empty row contributes no key yet — nothing to emit until filled.
	}

	function remove(localId: number): void {
		rows = rows.filter((row) => row.localId !== localId);
		emit();
	}
</script>

<div class="editor">
	{#each rows as row, index (row.localId)}
		<div class="row">
			<span class="ordinal">{index === 0 ? 'by' : 'then'}</span>
			<select
				class="field"
				aria-label="Sort field"
				value={row.field}
				onchange={(event: Event & { currentTarget: HTMLSelectElement }) => {
					update(row.localId, { field: event.currentTarget.value });
				}}
			>
				<option value="">Select field…</option>
				<option value="id">id</option>
				{#each schemaStore.fields as field (field.name)}
					<option value={field.name}>{field.name}</option>
				{/each}
			</select>
			<select
				class="direction"
				aria-label="Sort direction"
				value={row.direction}
				onchange={(event: Event & { currentTarget: HTMLSelectElement }) => {
					update(row.localId, { direction: event.currentTarget.value as SortDirection });
				}}
			>
				<option value="asc">ascending</option>
				<option value="desc">descending</option>
			</select>
			<button
				type="button"
				class="remove"
				aria-label="Remove sort key"
				onclick={() => {
					remove(row.localId);
				}}>×</button
			>
		</div>
	{/each}

	{#if rows.length === 0}
		<p class="hint">No sort — items in id order.</p>
	{/if}

	<div class="add-row">
		<button type="button" class="action" onclick={add}>+ Add sort key</button>
	</div>
</div>

<style>
	.editor {
		display: flex;
		flex-direction: column;
		gap: var(--space-2);
	}

	.row {
		display: flex;
		align-items: center;
		gap: var(--space-2);
	}

	.ordinal {
		min-width: 2.5rem;
		font-size: var(--text-sm);
		color: var(--color-fg-muted);
		text-align: right;
	}

	select {
		padding: 0.25rem var(--space-2);
		background-color: var(--color-bg);
		color: var(--color-fg);
		border: 1px solid var(--color-border);
		border-radius: var(--radius-sm);
		font-size: var(--text-sm);
	}

	.field {
		flex: 1;
	}

	.remove {
		background: none;
		border: 1px solid transparent;
		border-radius: var(--radius-sm);
		color: var(--color-fg-muted);
		font-size: var(--text-base);
		line-height: 1;
		padding: 0.125rem 0.375rem;
		cursor: pointer;
	}

	.remove:hover {
		color: var(--color-fg);
		border-color: var(--color-border);
	}

	.add-row {
		display: flex;
		gap: var(--space-2);
	}

	.action {
		background-color: var(--color-bg);
		color: var(--color-fg);
		border: 1px solid var(--color-border);
		border-radius: var(--radius-sm);
		padding: 0.25rem var(--space-2);
		font-size: var(--text-sm);
		cursor: pointer;
	}

	.action:hover {
		border-color: var(--color-accent);
	}

	.hint {
		color: var(--color-fg-muted);
		font-size: var(--text-sm);
		margin: 0;
	}
</style>
