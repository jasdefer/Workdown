<!--
  Compose and create a new view: name → kind → the slots that kind needs
  (field pickers constrained by schema metadata) → an optional filter
  (reusing FilterBuilder) → Save. Save is gated until the name and required
  slots are filled; the server re-validates and any diagnostics surface.
  On success we navigate to the new view.
-->
<script lang="ts">
	import { onMount } from 'svelte';
	import { goto } from '$app/navigation';
	import type { Clause } from '$lib/api/generated/Clause';
	import type { ViewType } from '$lib/api/generated/ViewType';
	import { api } from '$lib/api/client';
	import { schemaStore } from '$lib/stores/schema.svelte';
	import FilterBuilder from '$lib/filters/FilterBuilder.svelte';
	import {
		AGGREGATES,
		BUCKETS,
		VIEW_KINDS,
		VIEW_KIND_CONTROLS,
		WEEKDAYS,
		fieldFits,
		isDefinitionComplete,
		kindLabel
	} from './viewKinds';

	interface MetricRow {
		label: string;
		aggregate: string;
		value: string;
	}

	let name = $state('');
	let kind = $state<ViewType>('board');
	let definition = $state<Record<string, unknown>>({});
	let filterClauses = $state<Clause[]>([]);
	let ganttMode = $state<'end' | 'duration' | 'after'>('end');
	let metricRows = $state<MetricRow[]>([{ label: '', aggregate: 'count', value: '' }]);
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

	function chooseKind(next: ViewType): void {
		kind = next;
		definition = {};
		ganttMode = 'end';
		metricRows = [{ label: '', aggregate: 'count', value: '' }];
		syncMetrics();
	}

	// ── Gantt input mode ────────────────────────────────────────────────

	function setGanttMode(mode: 'end' | 'duration' | 'after'): void {
		ganttMode = mode;
		const next = { ...definition };
		delete next.end;
		delete next.duration;
		delete next.after;
		definition = next;
	}

	// ── Metric rows ─────────────────────────────────────────────────────

	function syncMetrics(): void {
		if (kind !== 'metric') return;
		const rows = metricRows.map((row) => {
			const entry: Record<string, unknown> = { aggregate: row.aggregate };
			if (row.label.trim() !== '') entry.label = row.label.trim();
			if (row.value !== '') entry.value = row.value;
			return entry;
		});
		definition = { ...definition, metrics: rows };
	}

	function updateMetric(index: number, patch: Partial<MetricRow>): void {
		metricRows = metricRows.map((row, i) => (i === index ? { ...row, ...patch } : row));
		syncMetrics();
	}

	function addMetric(): void {
		metricRows = [...metricRows, { label: '', aggregate: 'count', value: '' }];
		syncMetrics();
	}

	function removeMetric(index: number): void {
		metricRows = metricRows.filter((_, i) => i !== index);
		syncMetrics();
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
		const payload = { type: kind, ...definition };
		const result = await api.createView({ name, definition: payload, filter: filterClauses });
		saving = false;
		if (result.error !== undefined) {
			error = result.error;
			return;
		}
		if (result.data) {
			await goto(`/views/${encodeURIComponent(result.data.view_id)}`);
		}
	}
</script>

<div class="create-view">
	<h1>New view</h1>

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
			<label class="row">
				<span class="label">Start *</span>
				<select
					value={scalar('start')}
					onchange={(event) => {
						setSlot('start', event.currentTarget.value);
					}}
				>
					<option value="">Select field…</option>
					{#each fieldOptions(['date']) as field (field.name)}
						<option value={field.name}>{field.name}</option>
					{/each}
				</select>
			</label>
			<div class="row">
				<span class="label">End by *</span>
				<div class="modes">
					<label
						><input
							type="radio"
							checked={ganttMode === 'end'}
							onchange={() => {
								setGanttMode('end');
							}}
						/> End date</label
					>
					<label
						><input
							type="radio"
							checked={ganttMode === 'duration'}
							onchange={() => {
								setGanttMode('duration');
							}}
						/> Duration</label
					>
					<label
						><input
							type="radio"
							checked={ganttMode === 'after'}
							onchange={() => {
								setGanttMode('after');
							}}
						/> After predecessors</label
					>
				</div>
			</div>
			{#if ganttMode === 'end'}
				<label class="row">
					<span class="label">End field *</span>
					<select
						value={scalar('end')}
						onchange={(event) => {
							setSlot('end', event.currentTarget.value);
						}}
					>
						<option value="">Select field…</option>
						{#each fieldOptions(['date']) as field (field.name)}
							<option value={field.name}>{field.name}</option>
						{/each}
					</select>
				</label>
			{:else}
				<label class="row">
					<span class="label">Duration field *</span>
					<select
						value={scalar('duration')}
						onchange={(event) => {
							setSlot('duration', event.currentTarget.value);
						}}
					>
						<option value="">Select field…</option>
						{#each fieldOptions(['duration']) as field (field.name)}
							<option value={field.name}>{field.name}</option>
						{/each}
					</select>
				</label>
				{#if ganttMode === 'after'}
					<label class="row">
						<span class="label">Predecessor link *</span>
						<select
							value={scalar('after')}
							onchange={(event) => {
								setSlot('after', event.currentTarget.value);
							}}
						>
							<option value="">Select field…</option>
							{#each fieldOptions(['link', 'links']) as field (field.name)}
								<option value={field.name}>{field.name}</option>
							{/each}
						</select>
					</label>
				{/if}
			{/if}
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
				<div class="metrics">
					{#each metricRows as row, index (index)}
						<div class="metric-row">
							<input
								type="text"
								placeholder="label (optional)"
								value={row.label}
								onchange={(event) => {
									updateMetric(index, { label: event.currentTarget.value });
								}}
							/>
							<select
								value={row.aggregate}
								onchange={(event) => {
									updateMetric(index, { aggregate: event.currentTarget.value });
								}}
							>
								{#each AGGREGATES as aggregate (aggregate)}
									<option value={aggregate}>{aggregate}</option>
								{/each}
							</select>
							<select
								value={row.value}
								onchange={(event) => {
									updateMetric(index, { value: event.currentTarget.value });
								}}
							>
								<option value="">— no value —</option>
								{#each fieldOptions(['integer', 'float', 'duration', 'date']) as field (field.name)}
									<option value={field.name}>{field.name}</option>
								{/each}
							</select>
							<button
								type="button"
								class="remove"
								aria-label="Remove metric"
								onclick={() => {
									removeMetric(index);
								}}>×</button
							>
						</div>
					{/each}
					<button type="button" class="ghost" onclick={addMetric}>+ Add metric</button>
				</div>
			</div>
		{/if}
	{/each}

	<div class="filter-section">
		<span class="label">Filter (optional)</span>
		<FilterBuilder
			initialClauses={[]}
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
			{saving ? 'Creating…' : 'Create view'}
		</button>
		<a class="cancel" href="/">Cancel</a>
	</div>
</div>

<style>
	.create-view {
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

	.modes {
		display: flex;
		flex-wrap: wrap;
		gap: var(--space-3);
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

	.metric-row input,
	.metric-row select {
		flex: 1;
		min-width: 0;
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
