<!--
  The two cells that name what a rule row talks about: through which
  relation ("this item", or `parent`, `children`, `depends_on`…), and
  which field. Picking `children` and `status` is the `children.status`
  the file holds; a many-valued relation with no field is the related
  items themselves, which a count or an is-set test can ask about.

  Owns no state: the reference is the value, every change goes up as
  the whole new reference. A field the schema no longer has is kept in
  the list, marked, so a stale rule can be opened and repaired rather
  than silently rewritten.
-->
<script lang="ts">
	import type { FieldDefinitionData } from '$lib/api/generated/FieldDefinitionData';
	import type { Anchor, Reference } from './ruleDraft';

	interface Props {
		reference: Reference;
		anchors: Anchor[];
		fields: Pick<FieldDefinitionData, 'name'>[];
		/** Names the row in the two selects' accessible labels. */
		label: string;
		disabled?: boolean;
		onchange: (reference: Reference) => void;
	}

	let { reference, anchors, fields, label, disabled = false, onchange }: Props = $props();

	const anchor = $derived(anchors.find((candidate) => candidate.name === reference.anchor) ?? null);
	const bareAllowed = $derived(anchor?.many === true);
	const strayField = $derived(
		reference.field !== null && !fields.some((field) => field.name === reference.field)
			? reference.field
			: null
	);
</script>

<select
	class="anchor"
	aria-label="{label}: relation"
	value={reference.anchor ?? ''}
	{disabled}
	onchange={(event) => {
		const anchorName = event.currentTarget.value;
		onchange({ anchor: anchorName === '' ? null : anchorName, field: reference.field });
	}}
>
	<option value="">this item</option>
	{#each anchors as candidate (candidate.name)}
		<option value={candidate.name}>{candidate.name}</option>
	{/each}
</select>
<select
	class="field"
	aria-label="{label}: field"
	value={reference.field ?? ''}
	{disabled}
	onchange={(event) => {
		const fieldName = event.currentTarget.value;
		onchange({ anchor: reference.anchor, field: fieldName === '' ? null : fieldName });
	}}
>
	<option value="">{bareAllowed ? 'the related items' : '— field —'}</option>
	{#each fields as field (field.name)}
		<option value={field.name}>{field.name}</option>
	{/each}
	{#if strayField !== null}
		<option value={strayField}>{strayField} (unknown)</option>
	{/if}
</select>

<style>
	select {
		padding: 0.25rem var(--space-2);
		background-color: var(--color-bg);
		color: var(--color-fg);
		border: 1px solid var(--color-border);
		border-radius: var(--radius-sm);
		font-size: var(--text-sm);
		font-family: var(--font-mono);
		min-width: 0;
	}

	.anchor {
		flex: 0 1 8rem;
	}

	.field {
		flex: 1 1 8rem;
	}
</style>
