<!--
  Compose a view: name → kind → the slots that kind needs (field pickers
  constrained by schema metadata) → an optional filter (reusing
  FilterBuilder) → Save. Save is gated until the name and required slots
  are filled; the server re-validates and any diagnostics surface.

  Two modes, one form. Without `initial` it creates: on save the view is
  POSTed and we navigate to it. With `initial` (the view's id plus the
  seed from `GET /api/views/{id}/definition`) it edits: the form starts
  from the persisted definition and PUTs the replacement. The name field
  seeds from the prettified id — name → id is lossy — and is sent only
  when actually changed, so an untouched name never causes a rename.
-->
<script lang="ts">
	import { onMount, untrack } from 'svelte';
	import { goto } from '$app/navigation';
	import type { Clause } from '$lib/api/generated/Clause';
	import type { ViewType } from '$lib/api/generated/ViewType';
	import { api } from '$lib/api/client';
	import { schemaStore } from '$lib/stores/schema.svelte';
	import FilterBuilder from '$lib/filters/FilterBuilder.svelte';
	import GanttInput from './GanttInput.svelte';
	import MetricRowsEditor from './MetricRowsEditor.svelte';
	import { prettifyId } from './prettify';
	import {
		AGGREGATES,
		BUCKETS,
		VIEW_KINDS,
		VIEW_KIND_CONTROLS,
		WEEKDAYS,
		fieldFits,
		isDefinitionComplete,
		isRecord,
		kindLabel
	} from './viewKinds';

	interface Props {
		/** Present in edit mode: the view being edited and its persisted state. */
		initial?: { id: string; definition: Record<string, unknown>; filter: Clause[] };
	}

	let { initial }: Props = $props();

	/**
	 * Split a persisted flat definition into the form's working parts:
	 * the kind, the editable slots (with `display.fields` folded into the
	 * form-local `columns` slot), and the display roles the form does not
	 * edit (title, subtitle, color) — carried through untouched so an
	 * edit round-trip never drops what was set by hand in `views.yaml`.
	 * An explicitly empty `fields: []` (the documented "show no fields"
	 * override, distinct from unset) is not editable as columns either;
	 * it stays in the carried-through part so it survives a save.
	 */
	function seedFromDefinition(source: Record<string, unknown> | undefined): {
		kind: ViewType;
		slots: Record<string, unknown>;
		displayRest: Record<string, unknown>;
	} {
		if (source === undefined) return { kind: 'board', slots: {}, displayRest: {} };
		const { type, display, ...slots } = source;
		const displayRecord = isRecord(display) ? display : {};
		const { fields, ...displayRest } = displayRecord;
		if (Array.isArray(fields) && fields.length > 0) {
			slots.columns = fields;
		} else if (Array.isArray(fields)) {
			(displayRest as Record<string, unknown>).fields = fields;
		}
		return { kind: type as ViewType, slots, displayRest };
	}

	// Seeded once from the resolved prop — the host re-keys this component
	// when the edited view changes, so these deliberately don't track.
	const seeded = untrack(() => seedFromDefinition(initial?.definition));
	// The name a saved edit compares against: sending it unchanged would
	// be a no-op rename at best and an accidental one at worst.
	const initialName = untrack(() => (initial !== undefined ? prettifyId(initial.id) : ''));
	// The filter seed, resolved once and used both as the working state's
	// start value and as the builder's seed, so the two cannot diverge.
	const initialFilter = untrack(() => initial?.filter ?? []);

	let name = $state(initialName);
	let kind = $state<ViewType>(seeded.kind);
	let definition = $state<Record<string, unknown>>(seeded.slots);
	const displayRest = seeded.displayRest;
	let filterClauses = $state<Clause[]>(initialFilter);
	let saving = $state(false);
	let error = $state<string | null>(null);

	onMount(() => {
		void schemaStore.load();
	});

	const controls = $derived(VIEW_KIND_CONTROLS[kind]);
	const complete = $derived(name.trim() !== '' && isDefinitionComplete(kind, definition));

	function scalar(key: string): string {
		return typeof definition[key] === 'string' ? definition[key] : '';
	}

	function list(key: string): string[] {
		return Array.isArray(definition[key]) ? (definition[key] as string[]) : [];
	}

	function setSlot(key: string, value: string | string[] | undefined): void {
		if (value === undefined || value === '' || (Array.isArray(value) && value.length === 0)) {
			// Drop the key without a dynamic `delete`.
			definition = Object.fromEntries(
				Object.entries(definition).filter(([existing]) => existing !== key)
			);
		} else {
			definition = { ...definition, [key]: value };
		}
	}

	function onKindChange(event: Event & { currentTarget: HTMLSelectElement }): void {
		chooseKind(event.currentTarget.value as ViewType);
	}

	function fieldOptions(accepts: import('$lib/api/generated/FieldType').FieldType[]) {
		return schemaStore.fields.filter((field) => fieldFits(field.field_type, accepts));
	}

	// A kind switch resets the slots; a freshly mounted `MetricRowsEditor`
	// or `GanttInput` re-seeds its own part of the definition. Returning
	// to the seeded kind restores the persisted slots, so browsing other
	// kinds and coming back never loses them. The unedited display roles
	// are kept either way — they are kind-independent.
	function chooseKind(next: ViewType): void {
		kind = next;
		definition = next === seeded.kind ? { ...seeded.slots } : {};
	}

	// ── Working days ────────────────────────────────────────────────────

	function toggleWorkingDay(day: string, checked: boolean): void {
		const set = new Set(list('working_days'));
		if (checked) set.add(day);
		else set.delete(day);
		setSlot('working_days', [...set]);
	}

	// ── Save ────────────────────────────────────────────────────────────

	async function save(): Promise<void> {
		saving = true;
		error = null;
		// The form keeps `columns` as a flat slot for editing ergonomics;
		// on the wire it is the `fields` display role inside `display:` —
		// merged over the roles the form doesn't edit.
		const { columns, ...slots } = definition;
		const payload: Record<string, unknown> = { type: kind, ...slots };
		const display: Record<string, unknown> = { ...displayRest };
		if (Array.isArray(columns) && columns.length > 0) {
			display.fields = columns;
		}
		if (Object.keys(display).length > 0) {
			payload.display = display;
		}
		const result =
			initial !== undefined
				? await api.updateView(initial.id, {
						name: name !== initialName ? name : null,
						definition: payload,
						filter: filterClauses
					})
				: await api.createView({ name, definition: payload, filter: filterClauses });
		saving = false;
		if (result.error !== undefined) {
			error = result.error;
			return;
		}
		if (result.data) {
			await goto(`/views/${encodeURIComponent(result.data.view_id)}`, {
				invalidateAll: true
			});
		}
	}
</script>

<div class="view-form">
	<h1>{initial !== undefined ? 'Edit view' : 'New view'}</h1>

	<label class="row">
		<span class="label">Name</span>
		<input type="text" bind:value={name} placeholder="e.g. Open issues board" />
	</label>

	<label class="row">
		<span class="label">Kind</span>
		<select value={kind} onchange={onKindChange}>
			{#each VIEW_KINDS as option (option)}
				<option value={option}>{kindLabel(option)}</option>
			{/each}
		</select>
	</label>

	{#each controls as control (control.control + ('key' in control ? control.key : ''))}
		{#if control.control === 'field'}
			<label class="row">
				<span class="label">{control.label}{control.optional ? '' : ' *'}</span>
				<select
					value={scalar(control.key)}
					onchange={(event) => {
						setSlot(control.key, event.currentTarget.value);
					}}
				>
					<option value="">{control.optional ? '— none —' : 'Select field…'}</option>
					{#each fieldOptions(control.accepts) as field (field.name)}
						<option value={field.name}>{field.name}</option>
					{/each}
				</select>
			</label>
		{:else if control.control === 'fieldList'}
			<div class="row">
				<span class="label">{control.label}{control.optional ? '' : ' *'}</span>
				<select
					multiple
					size={Math.min(schemaStore.fields.length + 1, 8)}
					onchange={(event) => {
						setSlot(
							control.key,
							[...event.currentTarget.selectedOptions].map((o) => o.value)
						);
					}}
				>
					<option value="id" selected={list(control.key).includes('id')}>id</option>
					{#each schemaStore.fields as field (field.name)}
						<option value={field.name} selected={list(control.key).includes(field.name)}
							>{field.name}</option
						>
					{/each}
				</select>
			</div>
		{:else if control.control === 'aggregate'}
			<label class="row">
				<span class="label">{control.label} *</span>
				<select
					value={scalar(control.key)}
					onchange={(event) => {
						setSlot(control.key, event.currentTarget.value);
					}}
				>
					<option value="">Select…</option>
					{#each AGGREGATES as aggregate (aggregate)}
						<option value={aggregate}>{aggregate}</option>
					{/each}
				</select>
			</label>
		{:else if control.control === 'bucket'}
			<label class="row">
				<span class="label">{control.label}</span>
				<select
					value={scalar(control.key)}
					onchange={(event) => {
						setSlot(control.key, event.currentTarget.value);
					}}
				>
					<option value="">— none —</option>
					{#each BUCKETS as bucket (bucket)}
						<option value={bucket}>{bucket}</option>
					{/each}
				</select>
			</label>
		{:else if control.control === 'ganttInput'}
			<GanttInput {definition} onslot={setSlot} />
		{:else if control.control === 'workingDays'}
			<div class="row">
				<span class="label">{control.label}</span>
				<div class="checks">
					{#each WEEKDAYS as day (day)}
						<label class="check">
							<input
								type="checkbox"
								checked={list('working_days').includes(day)}
								onchange={(event) => {
									toggleWorkingDay(day, event.currentTarget.checked);
								}}
							/>
							{day.slice(0, 3)}
						</label>
					{/each}
				</div>
			</div>
		{:else if control.control === 'metrics'}
			<div class="row">
				<span class="label">Metrics *</span>
				<MetricRowsEditor
					initial={Array.isArray(definition.metrics)
						? (definition.metrics as Record<string, unknown>[])
						: undefined}
					onchange={(metrics) => {
						definition = { ...definition, metrics };
					}}
				/>
			</div>
		{/if}
	{/each}

	<div class="filter-section">
		<span class="label">Filter (optional)</span>
		<FilterBuilder
			initialClauses={initialFilter}
			onchange={(clauses: Clause[]) => {
				filterClauses = clauses;
			}}
		/>
	</div>

	{#if error !== null}
		<p class="error" role="alert">{error}</p>
	{/if}

	<div class="actions">
		<button type="button" class="primary" disabled={!complete || saving} onclick={save}>
			{#if initial !== undefined}
				{saving ? 'Saving…' : 'Save changes'}
			{:else}
				{saving ? 'Creating…' : 'Create view'}
			{/if}
		</button>
		<a
			class="cancel"
			href={initial !== undefined ? `/views/${encodeURIComponent(initial.id)}` : '/'}>Cancel</a
		>
	</div>
</div>

<style>
	.view-form {
		display: flex;
		flex-direction: column;
		gap: var(--space-3);
		max-width: 40rem;
		padding: var(--space-4);
	}

	h1 {
		font-size: var(--text-lg);
		margin: 0;
	}

	.row {
		display: flex;
		flex-direction: column;
		gap: var(--space-1);
	}

	.label {
		font-size: var(--text-sm);
		font-weight: 600;
		color: var(--color-fg-muted);
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

	.checks {
		display: flex;
		flex-wrap: wrap;
		gap: var(--space-2);
	}

	.check {
		display: inline-flex;
		align-items: center;
		gap: 0.25rem;
		font-size: var(--text-sm);
	}

	.filter-section {
		display: flex;
		flex-direction: column;
		gap: var(--space-2);
		padding-top: var(--space-2);
		border-top: 1px solid var(--color-border);
	}

	.actions {
		display: flex;
		align-items: center;
		gap: var(--space-3);
		padding-top: var(--space-2);
	}

	.primary {
		background-color: var(--color-accent);
		color: var(--color-accent-fg);
		border: 1px solid var(--color-accent);
		border-radius: var(--radius-sm);
		padding: 0.35rem var(--space-4);
		font-size: var(--text-sm);
		font-weight: 600;
		cursor: pointer;
	}

	.primary:disabled {
		opacity: 0.5;
		cursor: default;
	}

	.cancel {
		color: var(--color-fg-muted);
		font-size: var(--text-sm);
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
