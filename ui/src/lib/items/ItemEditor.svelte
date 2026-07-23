<!--
  The item editing surface, shared by the slide-over panel and the
  standalone `/items/:id` page. Self-sufficient: given an id it fetches
  the item (`GET /api/items/:id`) and the schema (once, cached), renders
  a per-field editor for every schema field, and shows the body
  read-only.

  Each field edit POSTs through `api.setField` and, on success, refetches
  the item so computed/cascading values reflect reality (and tells the
  host via `onmutate` so an underlying view can refresh too). Hard
  failures show an inline error; save-with-warning successes show the
  returned diagnostics — the file was still written.
-->
<script lang="ts">
	import { api } from '$lib/api/client';
	import type { Diagnostic } from '$lib/api/generated/Diagnostic';
	import type { FieldMutation } from '$lib/api/generated/FieldMutation';
	import type { FieldValue } from '$lib/api/generated/FieldValue';
	import type { ItemDetail } from '$lib/api/generated/ItemDetail';
	import { schemaStore } from '$lib/stores/schema.svelte';
	import DiagnosticList from '$lib/ui/DiagnosticList.svelte';
	import Markdown from '$lib/ui/Markdown.svelte';
	import { prettifyId } from '$lib/views/prettify';
	import FieldEditor from './FieldEditor.svelte';

	interface Props {
		itemId: string;
		/** Called after a successful mutation so a host view can refetch. */
		onmutate?: () => void;
	}

	let { itemId, onmutate }: Props = $props();

	let item = $state<ItemDetail | null>(null);
	let loadError = $state<string | null>(null);
	let actionError = $state<string | null>(null);
	let warnings = $state<Diagnostic[]>([]);
	let busy = $state(false);

	async function loadItem(id: string): Promise<void> {
		item = null;
		loadError = null;
		const result = await api.getItem(id);
		if (result.data !== undefined) {
			item = result.data;
		} else if (result.status === 404) {
			loadError = `Item '${id}' not found.`;
		} else {
			loadError = result.error ?? 'Failed to load item.';
		}
	}

	// Refetch whenever the target item changes; ensure the schema is loaded.
	$effect(() => {
		void schemaStore.load();
		void loadItem(itemId);
	});

	function valueOf(name: string): FieldValue | null {
		return item?.fields.find((field) => field.name === name)?.value ?? null;
	}

	async function commit(field: string, mutation: FieldMutation): Promise<void> {
		busy = true;
		actionError = null;
		const result = await api.setField(itemId, field, mutation);
		busy = false;
		if (result.error !== undefined) {
			actionError = result.error;
			return;
		}
		warnings = result.diagnostics;
		await loadItem(itemId);
		onmutate?.();
	}

	// `id` is the immutable identity (shown as the header), not an editor.
	const editableFields = $derived(schemaStore.fields.filter((field) => field.name !== 'id'));
</script>

<div
	class="item-editor"
	class:tinted={item !== null && item.background !== null}
	style:--item-color={item?.background}
>
	<header>
		<h2>{prettifyId(itemId)}</h2>
		<code>{itemId}</code>
	</header>

	{#if loadError}
		<p class="error">{loadError}</p>
	{:else if item === null}
		<p class="muted">Loading…</p>
	{:else}
		{#if actionError}
			<p class="error" role="alert">{actionError}</p>
		{/if}

		<DiagnosticList diagnostics={warnings} label="Warnings from the last change" />

		<dl class="fields card">
			{#each editableFields as field (field.name)}
				<dt>
					<span
						class="field-name"
						class:has-help={field.description !== null}
						title={field.description}>{field.name}</span
					>
					{#if field.required}<span class="req" title="required">*</span>{/if}
					{#if field.aggregate}<span class="note">computed</span>{/if}
				</dt>
				<dd>
					<FieldEditor
						{field}
						value={valueOf(field.name)}
						items={schemaStore.items}
						palette={schemaStore.palette}
						disabled={busy}
						oncommit={(mutation: FieldMutation) => {
							void commit(field.name, mutation);
						}}
					/>
					{#if field.resource}<small class="muted">values from “{field.resource}”</small>{/if}
				</dd>
			{/each}
		</dl>

		<section class="body card">
			<h3>Description</h3>
			{#if item.body.trim().length > 0}
				<Markdown content={item.body} />
			{:else}
				<p class="muted">No description. Edit the body in your editor.</p>
			{/if}
		</section>
	{/if}
</div>

<style>
	.item-editor {
		display: flex;
		flex-direction: column;
		gap: var(--space-3);
	}

	header {
		display: flex;
		flex-direction: column;
		gap: 0.15rem;
	}

	h2 {
		font-size: var(--text-lg);
		font-weight: 600;
		margin: 0;
	}

	header code {
		font-family: var(--font-mono);
		font-size: 0.8em;
		color: var(--color-fg-muted);
	}

	.card {
		/* `--card-bg` is the hook the item's `color` field fills via the
		   `tinted` class below; defaults to the neutral card surface. */
		background: var(--card-bg, var(--color-card));
		border: 1px solid var(--color-border);
		border-radius: var(--radius-md);
		box-shadow: var(--shadow-sm);
		padding: var(--space-4);
	}

	/* Stripe + tint — the same treatment as board cards and table rows,
	   so an item reads as its color on every surface. The wash flows to
	   both card surfaces through the --card-bg hook; the stripe carries
	   the full-strength hue. */
	.item-editor.tinted {
		--card-bg: color-mix(in srgb, var(--item-color) var(--tint-strength), var(--color-card));
	}

	.item-editor.tinted .card {
		border-left: 4px solid var(--item-color);
	}

	.fields {
		display: grid;
		grid-template-columns: minmax(6rem, 9rem) 1fr;
		gap: var(--space-2) var(--space-3);
		margin: 0;
		align-items: start;
	}

	dt {
		display: flex;
		align-items: baseline;
		gap: 0.25rem;
		font-size: var(--text-sm);
		color: var(--color-fg-muted);
		padding-top: 0.3rem;
	}

	dd {
		margin: 0;
		display: flex;
		flex-direction: column;
		gap: 0.15rem;
	}

	.field-name.has-help {
		text-decoration: underline dotted;
		text-underline-offset: 2px;
		cursor: help;
	}

	.req {
		color: var(--color-error-fg);
	}

	.note {
		font-size: 0.7em;
		text-transform: uppercase;
		letter-spacing: 0.04em;
		color: var(--color-fg-muted);
		border: 1px solid var(--color-border);
		border-radius: var(--radius-sm);
		padding: 0 0.25rem;
	}

	.body h3 {
		font-size: var(--text-sm);
		font-weight: 600;
		margin: 0 0 var(--space-1);
	}

	.muted {
		color: var(--color-fg-muted);
		font-size: var(--text-sm);
	}

	.error {
		color: var(--color-error-fg);
		font-size: var(--text-sm);
	}
</style>
