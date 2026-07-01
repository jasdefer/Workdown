<!--
  Filter editor — a bar that slides down beneath the nav, above the view.

  It owns the draft filter (`rows`) and is the single source of truth while
  open. Editing previews live: the draft is serialized to the `?filter=`
  URL param (debounced, `replaceState`), which the page loader passes to
  the server to re-extract the view without persisting — so the result
  below stays live. "Save" writes the filter to views.yaml via PATCH;
  "Reset" drops the preview and returns to the saved filter.

  Reused on every view page, and later by view creation. Keyed by view id
  upstream so it re-seeds when the user switches views.
-->
<script lang="ts">
	import { onMount } from 'svelte';
	import { slide } from 'svelte/transition';
	import { goto, invalidateAll } from '$app/navigation';
	import type { Clause } from '$lib/api/generated/Clause';
	import { schemaStore } from '$lib/stores/schema.svelte';
	import { api } from '$lib/api/client';
	import FilterRow from './FilterRow.svelte';
	import { clausesEqual, clausesToRows, rowsToClauses, type GuidedRow, type Row } from './clauses';

	interface Props {
		viewId: string;
		/** The `?filter=` param at load — a JSON clause array for a shared preview. */
		initialFilter: string | null;
		/** Preserved across preview/save navigations so the item panel stays open. */
		itemId: string | null;
	}

	let { viewId, initialFilter, itemId }: Props = $props();

	let rows = $state<Row[]>([]);
	let savedClauses = $state<Clause[]>([]);
	let seeded = $state(false);
	let expanded = $state(false);
	let saving = $state(false);
	let saveError = $state<string | null>(null);

	let idCounter = 0;
	const nextId = (): number => (idCounter += 1);

	const draftClauses = $derived(rowsToClauses(rows));
	const unsaved = $derived(seeded && !clausesEqual(draftClauses, savedClauses));

	function parseInitialFilter(raw: string | null): Clause[] | null {
		if (raw === null) return null;
		try {
			const parsed: unknown = JSON.parse(raw);
			return Array.isArray(parsed) ? (parsed as Clause[]) : null;
		} catch {
			return null;
		}
	}

	onMount(async () => {
		await schemaStore.load();
		const result = await api.getViewFilter(viewId);
		savedClauses = result.data ?? [];
		// Seed from the shared-preview URL if present, else the saved filter.
		const seed = parseInitialFilter(initialFilter) ?? savedClauses;
		rows = clausesToRows(seed, nextId);
		// A shared preview URL means we opened mid-narrowing — show the editor.
		if (initialFilter !== null) expanded = true;
		seeded = true;
	});

	// ── Preview (debounced navigation to ?filter=) ──────────────────────

	let previewTimer: ReturnType<typeof setTimeout> | undefined;

	function buildUrl(filterJson: string | null): string {
		const params = new URLSearchParams();
		if (itemId !== null) params.set('item', itemId);
		if (filterJson !== null) params.set('filter', filterJson);
		const query = params.toString();
		return `/views/${encodeURIComponent(viewId)}${query ? `?${query}` : ''}`;
	}

	function schedulePreview(): void {
		clearTimeout(previewTimer);
		previewTimer = setTimeout(() => {
			// Always reflect the draft — including empty (which previews
			// "show everything"), distinct from having no preview at all.
			const filterJson = JSON.stringify(rowsToClauses(rows));
			void goto(buildUrl(filterJson), { replaceState: true, keepFocus: true, noScroll: true });
		}, 300);
	}

	// ── Row edits ───────────────────────────────────────────────────────

	function addCondition(): void {
		const row: GuidedRow = {
			localId: nextId(),
			kind: 'comparison',
			field: '',
			operator: '',
			value: null
		};
		rows = [...rows, row];
		expanded = true;
	}

	function addRaw(): void {
		rows = [...rows, { localId: nextId(), kind: 'raw', raw: '' }];
		expanded = true;
	}

	function updateRow(updated: Row): void {
		rows = rows.map((row) => (row.localId === updated.localId ? updated : row));
		schedulePreview();
	}

	function removeRow(localId: number): void {
		rows = rows.filter((row) => row.localId !== localId);
		schedulePreview();
	}

	// ── Save / reset ────────────────────────────────────────────────────

	async function save(): Promise<void> {
		saving = true;
		saveError = null;
		const clauses = rowsToClauses(rows);
		const result = await api.patchViewFilter(viewId, clauses);
		saving = false;
		if (result.error !== undefined) {
			saveError = result.error;
			return;
		}
		savedClauses = clauses; // new baseline — `unsaved` clears
		// Drop the preview param and reload the now-persisted view. Any
		// save-with-warning diagnostics surface via the page's banner.
		await goto(buildUrl(null), { keepFocus: true, noScroll: true });
		await invalidateAll();
	}

	function reset(): void {
		saveError = null;
		rows = clausesToRows(savedClauses, nextId);
		void goto(buildUrl(null), { keepFocus: true, noScroll: true });
	}
</script>

<div class="filter-bar">
	<div class="header">
		<button
			type="button"
			class="toggle"
			aria-expanded={expanded}
			onclick={() => (expanded = !expanded)}
		>
			<span class="chevron" class:open={expanded}>▸</span>
			Filter
			{#if draftClauses.length > 0}<span class="count">{draftClauses.length}</span>{/if}
		</button>

		{#if unsaved}
			<span class="unsaved" in:slide={{ axis: 'x' }}>Previewing · unsaved</span>
			<button type="button" class="action save" disabled={saving} onclick={save}>
				{saving ? 'Saving…' : 'Save to view'}
			</button>
			<button type="button" class="action" disabled={saving} onclick={reset}>Reset</button>
		{/if}
	</div>

	{#if expanded}
		<div class="panel" transition:slide>
			{#if !seeded}
				<p class="hint">Loading…</p>
			{:else}
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
					<p class="hint">No filter — this view shows every item.</p>
				{/if}

				<div class="add-row">
					<button type="button" class="action" onclick={addCondition}>+ Add condition</button>
					<button type="button" class="action ghost" onclick={addRaw}>+ Add raw condition</button>
				</div>

				{#if saveError !== null}
					<p class="error" role="alert">{saveError}</p>
				{/if}
			{/if}
		</div>
	{/if}
</div>

<style>
	.filter-bar {
		border: 1px solid var(--color-border);
		border-radius: var(--radius-md);
		background-color: var(--color-surface);
	}

	.header {
		display: flex;
		align-items: center;
		gap: var(--space-2);
		padding: var(--space-2) var(--space-3);
	}

	.toggle {
		display: inline-flex;
		align-items: center;
		gap: var(--space-2);
		background: none;
		border: none;
		color: var(--color-fg);
		cursor: pointer;
		font-size: var(--text-sm);
		font-weight: 600;
		padding: 0;
	}

	.chevron {
		display: inline-block;
		transition: transform 0.15s ease;
		color: var(--color-fg-muted);
	}

	.chevron.open {
		transform: rotate(90deg);
	}

	.count {
		display: inline-flex;
		align-items: center;
		justify-content: center;
		min-width: 1.25rem;
		height: 1.25rem;
		padding: 0 0.35rem;
		border-radius: var(--radius-full);
		background-color: var(--color-accent);
		color: var(--color-accent-fg);
		font-size: var(--text-sm);
		font-weight: 600;
	}

	.unsaved {
		margin-left: auto;
		color: var(--color-warning-fg);
		background-color: var(--color-warning-bg);
		padding: 0.1rem var(--space-2);
		border-radius: var(--radius-full);
		font-size: var(--text-sm);
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

	.action:hover:not(:disabled) {
		border-color: var(--color-accent);
	}

	.action:disabled {
		opacity: 0.6;
		cursor: default;
	}

	.action.save {
		background-color: var(--color-accent);
		color: var(--color-accent-fg);
		border-color: var(--color-accent);
	}

	.action.ghost {
		color: var(--color-fg-muted);
	}

	.panel {
		display: flex;
		flex-direction: column;
		gap: var(--space-2);
		padding: var(--space-3);
		border-top: 1px solid var(--color-border);
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

	.hint {
		color: var(--color-fg-muted);
		font-size: var(--text-sm);
		margin: 0;
	}

	.error {
		color: var(--color-error-fg);
		background-color: var(--color-error-bg);
		padding: var(--space-2);
		border-radius: var(--radius-sm);
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
