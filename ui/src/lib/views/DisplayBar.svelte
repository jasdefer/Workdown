<!--
  Per-session display-role override for one view — a collapsed bar in the
  same style as `FilterBar`, above the view. Title / subtitle pickers and
  an ordered fields multi-select. Changes apply immediately: the override
  is written to localStorage and the page data invalidated, so the view
  below re-renders with the override taking highest precedence server-side
  (over the view's `display:` block and the config defaults). "Clear"
  removes the override and returns to the configured roles. Nothing is
  ever written to views.yaml.
-->
<script lang="ts">
	import { onMount } from 'svelte';
	import { slide } from 'svelte/transition';
	import { invalidateAll } from '$app/navigation';
	import { schemaStore } from '$lib/stores/schema.svelte';
	import { isEmptyOverride, saveDisplayOverride, type DisplayOverride } from './displayOverride';

	interface Props {
		viewId: string;
		/** The override active at load, from localStorage (null = none). */
		initialOverride: DisplayOverride | null;
	}

	let { viewId, initialOverride }: Props = $props();

	let expanded = $state(false);
	let title = $state('');
	let subtitle = $state('');
	let fields = $state<string[]>([]);

	const overrideCount = $derived(
		(title !== '' ? 1 : 0) + (subtitle !== '' ? 1 : 0) + (fields.length > 0 ? 1 : 0)
	);

	// Seed once from the override active at load. The component is keyed
	// by view id upstream, so switching views remounts and re-seeds; the
	// local state is the source of truth afterwards.
	onMount(() => {
		void schemaStore.load();
		title = initialOverride?.title ?? '';
		subtitle = initialOverride?.subtitle ?? '';
		fields = initialOverride?.fields ?? [];
	});

	function currentOverride(): DisplayOverride {
		const override: DisplayOverride = {};
		if (title !== '') override.title = title;
		if (subtitle !== '') override.subtitle = subtitle;
		if (fields.length > 0) override.fields = fields;
		return override;
	}

	async function apply(): Promise<void> {
		const override = currentOverride();
		saveDisplayOverride(viewId, isEmptyOverride(override) ? null : override);
		await invalidateAll();
	}

	function onTitleChange(event: Event): void {
		title = (event.currentTarget as HTMLSelectElement).value;
		void apply();
	}

	function onSubtitleChange(event: Event): void {
		subtitle = (event.currentTarget as HTMLSelectElement).value;
		void apply();
	}

	function onFieldsChange(event: Event): void {
		const select = event.currentTarget as HTMLSelectElement;
		fields = [...select.selectedOptions].map((option) => option.value);
		void apply();
	}

	async function clear(): Promise<void> {
		title = '';
		subtitle = '';
		fields = [];
		saveDisplayOverride(viewId, null);
		await invalidateAll();
	}
</script>

<div class="display-bar">
	<div class="header">
		<button
			type="button"
			class="toggle"
			aria-expanded={expanded}
			onclick={() => (expanded = !expanded)}
		>
			<span class="chevron" class:open={expanded}>▸</span>
			Display
			{#if overrideCount > 0}<span class="count">{overrideCount}</span>{/if}
		</button>

		{#if overrideCount > 0}
			<span class="overridden" in:slide={{ axis: 'x' }}>Overridden · this browser only</span>
			<button type="button" class="action" onclick={clear}>Clear</button>
		{/if}
	</div>

	{#if expanded}
		<div class="panel" transition:slide>
			<label>
				<span>Title</span>
				<select value={title} onchange={onTitleChange}>
					<option value="">— configured —</option>
					{#each schemaStore.fields as field (field.name)}
						<option value={field.name}>{field.name}</option>
					{/each}
				</select>
			</label>
			<label>
				<span>Subtitle</span>
				<select value={subtitle} onchange={onSubtitleChange}>
					<option value="">— configured —</option>
					{#each schemaStore.fields as field (field.name)}
						<option value={field.name}>{field.name}</option>
					{/each}
				</select>
			</label>
			<label>
				<span>Fields</span>
				<select
					multiple
					size={Math.min(schemaStore.fields.length + 1, 8)}
					onchange={onFieldsChange}
				>
					{#each schemaStore.fields as field (field.name)}
						<option value={field.name} selected={fields.includes(field.name)}>{field.name}</option>
					{/each}
				</select>
				<span class="hint">None selected = the configured fields.</span>
			</label>
		</div>
	{/if}
</div>

<style>
	.display-bar {
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

	.overridden {
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

	.action:hover {
		border-color: var(--color-accent);
	}

	.panel {
		display: grid;
		grid-template-columns: repeat(auto-fit, minmax(14rem, 1fr));
		gap: var(--space-3);
		padding: var(--space-3);
		border-top: 1px solid var(--color-border);
	}

	label {
		display: flex;
		flex-direction: column;
		gap: var(--space-1);
		font-size: var(--text-sm);
	}

	label > span:first-child {
		font-weight: 600;
	}

	select {
		background-color: var(--color-bg);
		color: var(--color-fg);
		border: 1px solid var(--color-border);
		border-radius: var(--radius-sm);
		padding: 0.25rem var(--space-2);
		font-size: var(--text-sm);
	}

	.hint {
		color: var(--color-fg-muted);
	}
</style>
