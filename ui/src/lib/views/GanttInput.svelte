<!--
  The gantt family's shared inputs: a start field plus the mutually
  exclusive way a bar's end is derived — an end-date field, a duration
  field, or duration-after-predecessors. The mode is UI-only state owned
  here; the chosen fields are reported up via `onslot`, and switching
  mode clears the other modes' slots so the definition never mixes them.
  The server checks the finer input-mode rules on save.
-->
<script lang="ts">
	import type { FieldType } from '$lib/api/generated/FieldType';
	import { schemaStore } from '$lib/stores/schema.svelte';
	import { fieldFits } from './viewKinds';

	type GanttMode = 'end' | 'duration' | 'after';

	interface Props {
		/** The current definition draft — read-only, to show selections. */
		definition: Record<string, unknown>;
		/** Report a slot change; `undefined` drops the slot. */
		onslot: (key: string, value: string | undefined) => void;
	}

	let { definition, onslot }: Props = $props();

	let mode = $state<GanttMode>('end');

	function scalar(key: string): string {
		const value = definition[key];
		return typeof value === 'string' ? value : '';
	}

	function fieldOptions(accepts: FieldType[]) {
		return schemaStore.fields.filter((field) => fieldFits(field.field_type, accepts));
	}

	function setMode(next: GanttMode): void {
		mode = next;
		onslot('end', undefined);
		onslot('duration', undefined);
		onslot('after', undefined);
	}
</script>

<label class="row">
	<span class="label">Start *</span>
	<select
		value={scalar('start')}
		onchange={(event) => {
			onslot('start', event.currentTarget.value);
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
				checked={mode === 'end'}
				onchange={() => {
					setMode('end');
				}}
			/> End date</label
		>
		<label
			><input
				type="radio"
				checked={mode === 'duration'}
				onchange={() => {
					setMode('duration');
				}}
			/> Duration</label
		>
		<label
			><input
				type="radio"
				checked={mode === 'after'}
				onchange={() => {
					setMode('after');
				}}
			/> After predecessors</label
		>
	</div>
</div>

{#if mode === 'end'}
	<label class="row">
		<span class="label">End field *</span>
		<select
			value={scalar('end')}
			onchange={(event) => {
				onslot('end', event.currentTarget.value);
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
				onslot('duration', event.currentTarget.value);
			}}
		>
			<option value="">Select field…</option>
			{#each fieldOptions(['duration']) as field (field.name)}
				<option value={field.name}>{field.name}</option>
			{/each}
		</select>
	</label>
	{#if mode === 'after'}
		<label class="row">
			<span class="label">Predecessor link *</span>
			<select
				value={scalar('after')}
				onchange={(event) => {
					onslot('after', event.currentTarget.value);
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

<style>
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
</style>
