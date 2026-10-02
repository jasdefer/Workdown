<!--
  The three-way default control of the field editor: no default, a
  fixed value, or a generated one. A fixed value is entered with the
  same `ValueEditor` the item panel uses, built for the draft's type
  and settings; a generated one is picked from the generators the
  server lists as valid for the type, and that choice is hidden for a
  type with none. The control owns no state: the draft's default is the
  mode, and every change goes up through `onchange`.
-->
<script lang="ts">
	import type { FieldValue } from '$lib/api/generated/FieldValue';
	import type { Generator } from '$lib/api/generated/Generator';
	import type { PaletteColor } from '$lib/api/generated/PaletteColor';
	import type { ResourceOption } from '$lib/api/generated/ResourceOption';
	import type { ValueSpec } from '$lib/values/valueSpec';
	import ValueEditor from '$lib/values/ValueEditor.svelte';
	import type { DraftDefault } from './fieldDraft';

	interface Props {
		value: DraftDefault | null;
		/** What a fixed value may be — the draft's type and settings. */
		spec: ValueSpec;
		/** The generators valid for the draft's type; none hides the choice. */
		generators: Generator[];
		items: string[];
		palette: PaletteColor[];
		resourceOptions: ResourceOption[];
		disabled?: boolean;
		onchange: (next: DraftDefault | null) => void;
	}

	let {
		value,
		spec,
		generators,
		items,
		palette,
		resourceOptions,
		disabled = false,
		onchange
	}: Props = $props();

	type Mode = 'none' | 'literal' | 'generator';

	const mode = $derived<Mode>(value === null ? 'none' : value.kind);

	function onModeChange(event: Event & { currentTarget: HTMLSelectElement }): void {
		const next = event.currentTarget.value as Mode;
		if (next === 'none') onchange(null);
		else if (next === 'literal') onchange({ kind: 'literal', value: null });
		else if (generators[0] !== undefined) onchange({ kind: 'generator', generator: generators[0] });
	}

	function onGeneratorChange(event: Event & { currentTarget: HTMLSelectElement }): void {
		onchange({ kind: 'generator', generator: event.currentTarget.value as Generator });
	}

	function onLiteralChange(next: FieldValue | null): void {
		onchange({ kind: 'literal', value: next });
	}
</script>

<div class="default-control">
	<select aria-label="Default kind" {disabled} value={mode} onchange={onModeChange}>
		<option value="none">No default</option>
		<option value="literal">Fixed value</option>
		{#if generators.length > 0}
			<option value="generator">Generated</option>
		{/if}
	</select>

	{#if value?.kind === 'literal'}
		<div class="editor">
			<ValueEditor
				{spec}
				value={value.value}
				{items}
				{palette}
				{resourceOptions}
				{disabled}
				onchange={onLiteralChange}
			/>
		</div>
	{:else if value?.kind === 'generator'}
		<select aria-label="Generator" {disabled} value={value.generator} onchange={onGeneratorChange}>
			{#each generators as generator (generator)}
				<option value={generator}>{generator}</option>
			{/each}
		</select>
	{/if}
</div>

<style>
	.default-control {
		display: flex;
		flex-direction: column;
		gap: var(--space-2);
	}

	select {
		width: 100%;
		padding: 0.25rem var(--space-2);
		background-color: var(--color-bg);
		color: var(--color-fg);
		border: 1px solid var(--color-border);
		border-radius: var(--radius-sm);
		font-size: var(--text-sm);
	}

	.editor {
		padding-left: var(--space-3);
		border-left: 2px solid var(--color-border);
	}
</style>
