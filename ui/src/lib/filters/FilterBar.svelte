<!--
  Filter editor for an existing view — a `CollapsibleBar` beneath the
  nav, above the view. Wraps the reusable `FilterBuilder` with the
  view-specific behaviour: seed from the persisted filter, live-preview
  the draft via the `?filter=` URL param (debounced, `replaceState`) so
  the result below re-narrows without persisting, and Save / Reset.

  Keyed by view id upstream so it re-seeds when the user switches views.
-->
<script lang="ts">
	import { onMount } from 'svelte';
	import { goto, invalidateAll } from '$app/navigation';
	import type { Clause } from '$lib/api/generated/Clause';
	import { schemaStore } from '$lib/stores/schema.svelte';
	import { api } from '$lib/api/client';
	import CollapsibleBar, { type BarAction } from '$lib/ui/CollapsibleBar.svelte';
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

	const actions = $derived<BarAction[]>([
		{
			label: saving ? 'Saving…' : 'Save to view',
			onclick: () => void save(),
			primary: true,
			disabled: saving
		},
		{ label: 'Reset', onclick: reset, disabled: saving }
	]);

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

<CollapsibleBar
	label="Filter"
	count={draftClauses.length}
	status={unsaved ? 'Previewing · unsaved' : null}
	{actions}
	bind:expanded
>
	<div class="content">
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
</CollapsibleBar>

<style>
	.content {
		display: flex;
		flex-direction: column;
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
</style>
