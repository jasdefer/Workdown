<!--
  The reusable filter builder: a list of condition rows (guided + raw) with
  add/remove, emitting the complete clauses via `onchange`. It owns the row
  state but nothing view-specific — no seeding, preview, or persistence.

  `FilterBar` wraps it for an existing view (seed / live preview / save);
  the view-creation form embeds it to attach a filter at creation. Seeded
  once from `initialClauses` at mount, so the host resolves that value
  first (and re-keys the component to re-seed, e.g. on reset).
-->
<script lang="ts">
	import { untrack } from 'svelte';
	import type { Clause } from '$lib/api/generated/Clause';
	import FilterRow from './FilterRow.svelte';
	import { clausesToRows, rowsToClauses, type Row } from './clauses';

	interface Props {
		/** Clauses to seed the rows from, resolved by the host before mount. */
		initialClauses: Clause[];
		/** Fires with the complete clauses whenever an edit changes them. */
		onchange: (clauses: Clause[]) => void;
	}

	let { initialClauses, onchange }: Props = $props();

	let idCounter = 0;
	const nextId = (): number => (idCounter += 1);

	// Seeded once from the resolved prop; the host re-keys this component to
	// re-seed (e.g. on Reset), so this deliberately doesn't track changes.
	let rows = $state<Row[]>(untrack(() => clausesToRows(initialClauses, nextId)));

	function emit(): void {
		onchange(rowsToClauses(rows));
	}

	function addCondition(): void {
		rows = [
			...rows,
			{ localId: nextId(), kind: 'comparison', field: '', operator: '', value: null }
		];
		// An empty row contributes no clause yet — nothing to emit until filled.
	}

	function addRaw(): void {
		rows = [...rows, { localId: nextId(), kind: 'raw', raw: '' }];
	}

	function updateRow(updated: Row): void {
		rows = rows.map((row) => (row.localId === updated.localId ? updated : row));
		emit();
	}

	function removeRow(localId: number): void {
		rows = rows.filter((row) => row.localId !== localId);
		emit();
	}
</script>

<div class="builder">
	{#each rows as row (row.localId)}
		{#if row.kind === 'comparison'}
			<FilterRow
				{row}
				onchange={updateRow}
				onremove={() => {
					removeRow(row.localId);
				}}
			/>
		{:else}
			<div class="raw-row">
				<span class="raw-tag">raw</span>
				<input
					class="raw-input"
					type="text"
					placeholder="raw clause, e.g. status=open,done"
					value={row.raw}
					onchange={(event) => {
						updateRow({ ...row, raw: event.currentTarget.value });
					}}
				/>
				<button
					type="button"
					class="remove"
					aria-label="Remove clause"
					onclick={() => {
						removeRow(row.localId);
					}}>×</button
				>
			</div>
		{/if}
	{/each}

	{#if rows.length === 0}
		<p class="hint">No conditions — matches every item.</p>
	{/if}

	<div class="add-row">
		<button type="button" class="action" onclick={addCondition}>+ Add condition</button>
		<button type="button" class="action ghost" onclick={addRaw}>+ Add raw condition</button>
	</div>
</div>

<style>
	.builder {
		display: flex;
		flex-direction: column;
		gap: var(--space-2);
	}

	.raw-row {
		display: flex;
		align-items: center;
		gap: var(--space-2);
	}

	.raw-tag {
		font-size: var(--text-sm);
		color: var(--color-fg-muted);
		font-family: var(--font-mono);
	}

	.raw-input {
		flex: 1;
		padding: 0.25rem var(--space-2);
		background-color: var(--color-bg);
		color: var(--color-fg);
		border: 1px solid var(--color-border);
		border-radius: var(--radius-sm);
		font-size: var(--text-sm);
		font-family: var(--font-mono);
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

	.action.ghost {
		color: var(--color-fg-muted);
	}

	.hint {
		color: var(--color-fg-muted);
		font-size: var(--text-sm);
		margin: 0;
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
