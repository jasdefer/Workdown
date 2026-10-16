<!--
  The `resource:` picker of the field editor: which list in
  `resources.yaml` a value must be an entry of, or none. Shown for the
  types the server's table says take a resource (`string`, `list`); it
  is a property of the field, not of its shape, so it sits beside the
  shape block rather than inside it. The names come from the schema
  store, which already serves the lists' entries to the value pickers;
  this control only reads them, editing the lists is not its job.

  A name the file holds but no list carries — a list renamed or
  removed in `resources.yaml` — stays in the options, marked, so the
  panel shows what the file says and the next save does not silently
  drop it. The server reports such a name as a schema error either
  way.

  Owns no state: the draft's resource is the value, and a change goes
  up as the new name or `null` for none.
-->
<script lang="ts">
	interface Props {
		value: string | null;
		/** The lists `resources.yaml` declares, in file order. */
		names: string[];
		disabled?: boolean;
		onchange: (resource: string | null) => void;
	}

	let { value, names, disabled = false, onchange }: Props = $props();

	const stray = $derived(value !== null && !names.includes(value) ? value : null);

	function onSelect(event: Event & { currentTarget: HTMLSelectElement }): void {
		const picked = event.currentTarget.value;
		onchange(picked === '' ? null : picked);
	}
</script>

<label class="row">
	<span class="label">Resource</span>
	<select value={value ?? ''} {disabled} onchange={onSelect}>
		<option value="">—</option>
		{#each names as name (name)}
			<option value={name}>{name}</option>
		{/each}
		{#if stray !== null}
			<option value={stray}>{stray} (unknown)</option>
		{/if}
	</select>
	{#if stray !== null}
		<span class="hint warning">No list named <code>{stray}</code> in resources.yaml.</span>
	{:else if names.length === 0}
		<span class="hint">resources.yaml declares no lists yet, so there is nothing to pick.</span>
	{:else}
		<span class="hint">A value must be an entry of the list. Leave empty for any value.</span>
	{/if}
</label>

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
		width: 100%;
		padding: 0.25rem var(--space-2);
		background-color: var(--color-bg);
		color: var(--color-fg);
		border: 1px solid var(--color-border);
		border-radius: var(--radius-sm);
		font-size: var(--text-sm);
		font-family: inherit;
	}

	.hint {
		font-size: var(--text-sm);
		color: var(--color-fg-muted);
	}

	.hint code {
		background-color: transparent;
		padding: 0;
	}

	.hint.warning {
		color: var(--color-warning-fg);
	}
</style>
