<!--
  The `sort:` slot as rows of field → direction, with add/remove. Owns
  the row state and reports every edit up via `onchange` with the full
  list, so the view form stays the single source of truth for what gets
  saved. Seeded once from `initialKeys`; the host re-keys the component
  to re-seed.

  Every schema field is offered, plus the virtual `id` — the server
  accepts any field type for a sort key — so the picker needs no type
  filter.
-->
<script lang="ts">
	import { untrack } from 'svelte';
	import { schemaStore } from '$lib/stores/schema.svelte';
	import type { SortDirection, SortKey } from './sortSlot';

	interface Props {
		/** Keys to seed the rows from, resolved by the host before mount. */
		initialKeys: SortKey[];
		/** Fires with the complete key list whenever an edit changes it. */
		onchange: (keys: SortKey[]) => void;
	}

	let { initialKeys, onchange }: Props = $props();

	let idCounter = untrack(() => Math.max(0, ...initialKeys.map((key) => key.localId)));
	const nextId = (): number => (idCounter += 1);

	let keys = $state<SortKey[]>(untrack(() => [...initialKeys]));

	function emit(): void {
		onchange(keys);
	}

	function addKey(): void {
		keys = [...keys, { localId: nextId(), field: '', direction: 'asc' }];
		// An empty row contributes no key yet — nothing to emit until filled.
	}

	function updateKey(updated: SortKey): void {
		keys = keys.map((key) => (key.localId === updated.localId ? updated : key));
		emit();
	}

	function removeKey(localId: number): void {
		keys = keys.filter((key) => key.localId !== localId);
		emit();
	}
</script>

<div class="editor">
	{#each keys as key, index (key.localId)}
		<div class="row">
			<span class="ordinal">{index === 0 ? 'by' : 'then'}</span>
			<select
				class="field"
				aria-label="Sort field"
				value={key.field}
				onchange={(event: Event & { currentTarget: HTMLSelectElement }) => {
					updateKey({ ...key, field: event.currentTarget.value });
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
				value={key.direction}
				onchange={(event: Event & { currentTarget: HTMLSelectElement }) => {
					updateKey({ ...key, direction: event.currentTarget.value as SortDirection });
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
					removeKey(key.localId);
				}}>×</button
			>
		</div>
	{/each}

	{#if keys.length === 0}
		<p class="hint">No sort — items in id order.</p>
	{/if}

	<div class="add-row">
		<button type="button" class="action" onclick={addKey}>+ Add sort key</button>
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
