<!--
  Per-session display-role override for one view — a `CollapsibleBar`
  above the view, next to the filter bar. Title / subtitle pickers, an
  ordered fields multi-select, and — when the schema declares
  color-typed fields — a tint picker (any color field, "None" for no
  tint, or the configured resolution). Changes apply immediately: the
  override is written to localStorage and the page data invalidated, so
  the view below re-renders with the override taking highest precedence
  server-side (over the view's `display:` block and the config
  defaults). "Clear" removes the override and returns to the configured
  roles. Nothing is ever written to views.yaml.
-->
<script lang="ts">
	import { onMount } from 'svelte';
	import { invalidateAll } from '$app/navigation';
	import { schemaStore } from '$lib/stores/schema.svelte';
	import CollapsibleBar, { type BarAction } from '$lib/ui/CollapsibleBar.svelte';
	import { saveDisplayOverride, type DisplayOverride } from './displayOverride';

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
	// '' = configured, 'none' = the sentinel (no tint), else a field name.
	let color = $state('');

	const overrideCount = $derived(
		(title !== '' ? 1 : 0) +
			(subtitle !== '' ? 1 : 0) +
			(fields.length > 0 ? 1 : 0) +
			(color !== '' ? 1 : 0)
	);

	const actions = $derived<BarAction[]>([{ label: 'Clear', onclick: () => void clear() }]);

	// The color role only accepts color-typed fields; with none in the
	// schema there is nothing to switch, so the picker hides entirely.
	const colorFields = $derived(schemaStore.fields.filter((field) => field.field_type === 'color'));

	// Seed once from the override active at load. The component is keyed
	// by view id upstream, so switching views remounts and re-seeds; the
	// local state is the source of truth afterwards.
	onMount(() => {
		void schemaStore.load();
		title = initialOverride?.title ?? '';
		subtitle = initialOverride?.subtitle ?? '';
		fields = initialOverride?.fields ?? [];
		color = initialOverride?.color ?? '';
	});

	function currentOverride(): DisplayOverride {
		const override: DisplayOverride = {};
		if (title !== '') override.title = title;
		if (subtitle !== '') override.subtitle = subtitle;
		if (fields.length > 0) override.fields = fields;
		if (color !== '') override.color = color;
		return override;
	}

	// `saveDisplayOverride` removes the stored entry when the override is
	// empty, so applying an all-defaults state clears rather than saves.
	async function apply(): Promise<void> {
		saveDisplayOverride(viewId, currentOverride());
		await invalidateAll();
	}

	// The two text roles share one picker markup; `set` writes the role's
	// local state and applies.
	const textRoles = $derived([
		{
			label: 'Title',
			value: title,
			set: (value: string) => {
				title = value;
				void apply();
			}
		},
		{
			label: 'Subtitle',
			value: subtitle,
			set: (value: string) => {
				subtitle = value;
				void apply();
			}
		}
	]);

	function onFieldsChange(event: Event & { currentTarget: HTMLSelectElement }): void {
		// `selectedOptions` comes back in document order, so this override
		// can pick *which* fields show but not reorder them — a custom
		// order needs the view's `display.fields` in views.yaml.
		fields = [...event.currentTarget.selectedOptions].map((option) => option.value);
		void apply();
	}

	async function clear(): Promise<void> {
		title = '';
		subtitle = '';
		fields = [];
		color = '';
		saveDisplayOverride(viewId, null);
		await invalidateAll();
	}
</script>

<CollapsibleBar
	label="Display"
	count={overrideCount}
	status={overrideCount > 0 ? 'Overridden · this browser only' : null}
	{actions}
	bind:expanded
>
	<div class="controls">
		{#each textRoles as role (role.label)}
			<label>
				<span>{role.label}</span>
				<select
					value={role.value}
					onchange={(event) => {
						role.set(event.currentTarget.value);
					}}
				>
					<option value="">— configured —</option>
					{#each schemaStore.fields as field (field.name)}
						<option value={field.name}>{field.name}</option>
					{/each}
				</select>
			</label>
		{/each}
		{#if colorFields.length > 0}
			<label>
				<span>Color</span>
				<select
					value={color}
					onchange={(event) => {
						color = event.currentTarget.value;
						void apply();
					}}
				>
					<option value="">— configured —</option>
					<option value="none">None (no tint)</option>
					{#each colorFields as field (field.name)}
						<option value={field.name}>{field.name}</option>
					{/each}
				</select>
			</label>
		{/if}
		<label>
			<span>Fields</span>
			<select multiple size={Math.min(schemaStore.fields.length + 1, 8)} onchange={onFieldsChange}>
				<option value="id" selected={fields.includes('id')}>id</option>
				{#each schemaStore.fields as field (field.name)}
					<option value={field.name} selected={fields.includes(field.name)}>{field.name}</option>
				{/each}
			</select>
			<span class="hint">None selected = the configured fields.</span>
		</label>
	</div>
</CollapsibleBar>

<style>
	.controls {
		display: grid;
		grid-template-columns: repeat(auto-fit, minmax(14rem, 1fr));
		gap: var(--space-3);
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
