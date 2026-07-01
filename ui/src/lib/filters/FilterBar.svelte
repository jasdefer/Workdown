<!--
  Filter editor for an existing view — a bar that slides down beneath the
  nav, above the view. Wraps the reusable `FilterBuilder` with the
  view-specific behaviour: seed from the persisted filter, live-preview the
  draft via the `?filter=` URL param (debounced, `replaceState`) so the
  result below re-narrows without persisting, and Save / Reset.

  Keyed by view id upstream so it re-seeds when the user switches views.
-->
<script lang="ts">
	import { onMount } from 'svelte';
	import { slide } from 'svelte/transition';
	import { goto, invalidateAll } from '$app/navigation';
	import type { Clause } from '$lib/api/generated/Clause';
	import { schemaStore } from '$lib/stores/schema.svelte';
	import { api } from '$lib/api/client';
	import FilterBuilder from './FilterBuilder.svelte';
	import { clausesEqual } from './clauses';

	interface Props {
		viewId: string;
		/** The `?filter=` param at load — a JSON clause array for a shared preview. */
		initialFilter: string | null;
		/** Preserved across preview/save navigations so the item panel stays open. */
		itemId: string | null;
	}

	let { viewId, initialFilter, itemId }: Props = $props();

	let savedClauses = $state<Clause[]>([]);
	let draftClauses = $state<Clause[]>([]);
	let initialClauses = $state<Clause[]>([]);
	let seeded = $state(false);
	let builderKey = $state(0); // bump to re-seed the builder (Reset)
	let expanded = $state(false);
	let saving = $state(false);
	let saveError = $state<string | null>(null);

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
		initialClauses = seed;
		draftClauses = seed;
		if (initialFilter !== null) expanded = true;
		seeded = true;
	});

	// ── Live preview (debounced navigation to ?filter=) ─────────────────

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
			// Always reflect the draft — including empty (previews "show all").
			const filterJson = JSON.stringify(draftClauses);
			void goto(buildUrl(filterJson), { replaceState: true, keepFocus: true, noScroll: true });
		}, 300);
	}

	function handleChange(clauses: Clause[]): void {
		draftClauses = clauses;
		schedulePreview();
	}

	// ── Save / reset ────────────────────────────────────────────────────

	async function save(): Promise<void> {
		saving = true;
		saveError = null;
		const result = await api.patchViewFilter(viewId, draftClauses);
		saving = false;
		if (result.error !== undefined) {
			saveError = result.error;
			return;
		}
		savedClauses = draftClauses; // new baseline — `unsaved` clears
		initialClauses = draftClauses; // a later Reset returns here
		await goto(buildUrl(null), { keepFocus: true, noScroll: true });
		await invalidateAll();
	}

	function reset(): void {
		saveError = null;
		draftClauses = savedClauses;
		initialClauses = savedClauses;
		builderKey += 1; // re-seed the builder from the saved filter
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
				{#key builderKey}
					<FilterBuilder {initialClauses} onchange={handleChange} />
				{/key}
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

	.panel {
		display: flex;
		flex-direction: column;
		gap: var(--space-2);
		padding: var(--space-3);
		border-top: 1px solid var(--color-border);
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
</style>
