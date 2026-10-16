<!--
  The field editor: the slide-over the schema page opens on a row click
  (`?field=<name>`) or on Add field (`?add`). A header block every type
  shares — name, type, required, description, default — then the
  type-specific block, the "Value" section with the way the value is
  filled (by hand, by the first condition that holds, by a formula, or
  pulled from linked items), the rollup section for the types that
  take one, and beside them the YAML the save would write and what the
  field is used by. The shell — columns, YAML pane, footer — is
  `DraftPanel`, shared with the rule editor.

  The YAML pane and the pre-save verdict come from the server: on every
  pause in editing the draft goes to the preview endpoint, which
  answers with the entry the write would produce and whether the schema
  would load with it and keep the field. A problem it reports greys out
  Save: the person is right there, so a draft the load would disable is
  not written and warned about afterwards, as an item would be
  (`schema-derived-field-editor`, decision 10).

  The panel copies the field into a draft when it mounts and never
  reads the field again: the page behind it refetches on every file
  change in the project, and a form that reset on each would lose work
  to background writes. Switching to another field remounts the panel
  (the page keys it by name). A field removed meanwhile surfaces as the
  save's not-found answer.

  Saving writes directly. A definition that would not load comes back
  as the server's message in the panel and nothing is written; a write
  that loads but warns is written, and the warnings go to the page
  through `onsaved`. Remove is greyed out while anything whose break
  outlives a warning names the field — a view, a config role, a recipe,
  a rule — with those as the explanation. Items holding a value do not
  block; the confirmation offers to remove the field alone or to drop
  the values from those items as well.

  The type-specific part is `ShapeBlock`, dispatched on the shape the
  type is edited as, plus the `resource:` picker for the types that
  take one — a property of the field, not of its shape.
-->
<script lang="ts">
	import { onMount, untrack } from 'svelte';
	import { api } from '$lib/api/client';
	import type { AggregateConfig } from '$lib/api/generated/AggregateConfig';
	import type { Diagnostic } from '$lib/api/generated/Diagnostic';
	import type { FieldDefinitionData } from '$lib/api/generated/FieldDefinitionData';
	import type { FieldPreview } from '$lib/api/generated/FieldPreview';
	import type { FieldShape } from '$lib/api/generated/FieldShape';
	import type { FieldType } from '$lib/api/generated/FieldType';
	import type { PreviewField } from '$lib/api/generated/PreviewField';
	import type { PullConfig } from '$lib/api/generated/PullConfig';
	import type { SchemaDefinitionData } from '$lib/api/generated/SchemaDefinitionData';
	import { schemaStore } from '$lib/stores/schema.svelte';
	import ConfirmDialog from '$lib/ui/ConfirmDialog.svelte';
	import DraftPanel from '$lib/ui/DraftPanel.svelte';
	import { specOf } from '$lib/values/valueSpec';
	import AggregateBlock from './AggregateBlock.svelte';
	import ComputeBlock from './ComputeBlock.svelte';
	import DefaultControl from './DefaultControl.svelte';
	import PullBlock from './PullBlock.svelte';
	import ResourceControl from './ResourceControl.svelte';
	import ShapeBlock from './ShapeBlock.svelte';
	import UsageSection from './UsageSection.svelte';
	import WhenBlock from './WhenBlock.svelte';
	import {
		aggregateFunctionsFor,
		expressionReferences,
		generatorsFor,
		initialDraft,
		newDraft,
		newPull,
		newRollup,
		operandChoices,
		pullRelations,
		pullSources,
		retype,
		rollupRelations,
		shapeProblem,
		takesAggregate,
		takesCompute,
		takesPull,
		takesResource,
		takesWhen,
		valueKindOf,
		withValueKind,
		writeBody,
		type ComputeRecipe,
		type DraftDefault,
		type DroppedDefault,
		type ValueKind,
		type WhenRecipe
	} from './fieldDraft';
	import { blockerPhrase, removeDialogText, summarizeUsage, type UsageState } from './fieldUsage';
	import { createPreview } from './preview.svelte';
	import type { FieldPanelTarget } from './schemaPage';

	interface Props {
		definition: SchemaDefinitionData;
		target: FieldPanelTarget;
		onclose: () => void;
		/** The write happened; the host shows `warnings` and closes the panel. */
		onsaved: (warnings: Diagnostic[]) => void;
	}

	let { definition, target, onclose, onsaved }: Props = $props();

	// Read once, deliberately: the field as served when the panel opened.
	// Later refetches change `definition` behind the panel and must not
	// reach the draft.
	const existing: FieldDefinitionData | null = untrack(() =>
		target.kind === 'edit'
			? (definition.fields.find((field) => field.name === target.name) ?? null)
			: null
	);
	const missingName = untrack(() =>
		target.kind === 'edit' && existing === null ? target.name : null
	);

	let draft = $state(
		untrack(() => (existing === null ? newDraft(definition, 'string') : initialDraft(existing)))
	);
	let saving = $state(false);
	let saveError = $state<string | null>(null);
	let confirmingRemove = $state(false);
	let removeButton = $state<HTMLButtonElement>();
	let usage = $state<UsageState>({ status: 'loading' });
	// What the last change of value kind dropped, said beside the default
	// (decision 7).
	let dropped = $state<DroppedDefault>(null);

	const isId = existing?.name === 'id';
	// Every existing field keeps its type here; a change is made in
	// `schema.yaml`, where any pair is possible and judged at load. A
	// field with a recipe keeps it for good, the recipe was type-checked
	// against it.
	const typeLocked = existing !== null;

	const fieldTypes = $derived(definition.properties_by_type.map((row) => row.field_type));
	const generators = $derived(generatorsFor(definition, draft.fieldType));
	const nameValid = $derived(existing !== null || draft.name.trim() !== '');
	// What is wrong with the type-specific settings, when something the
	// browser can judge is; greys out Save with the reason.
	const shapeIssue = $derived(shapeProblem(draft.shape));
	const showsResource = $derived(takesResource(definition, draft.fieldType));
	const showsRollup = $derived(takesAggregate(definition, draft.fieldType));
	const rollupFunctions = $derived(aggregateFunctionsFor(definition, draft.fieldType));
	const rollupRelationNames = $derived(rollupRelations(definition.fields));
	const rollupStart = $derived(newRollup(definition, definition.fields, draft.fieldType));
	// The "Value" section: shown where the type can be derived at all.
	const computeAllowed = $derived(takesCompute(definition, draft.fieldType));
	const whenAllowed = $derived(takesWhen(definition, draft.fieldType));
	const showsValue = $derived(
		computeAllowed || whenAllowed || takesPull(definition, draft.fieldType)
	);
	const choices = $derived(
		operandChoices(definition, definition.fields, schemaStore.constants, existing?.name ?? null)
	);
	const pullRelationNames = $derived(pullRelations(definition.fields));
	const pullSourceFields = $derived(pullSources(definition, definition.fields, draft.fieldType));
	const pullStart = $derived(newPull(definition, definition.fields, draft.fieldType));
	const references = $derived(
		expressionReferences(
			definition,
			definition.fields,
			schemaStore.constants,
			existing?.name ?? null
		)
	);
	const valueKind = $derived(valueKindOf(draft));
	const spec = $derived(specOf(draft.fieldType, draft.shape));

	// A new field without a name has no entry to show yet; the pane says
	// so instead of relaying the server's refusal of the empty name.
	const previewRequest = $derived.by((): PreviewField | null => {
		const name = existing?.name ?? draft.name.trim();
		if (name === '') return null;
		return { name, replacing: existing !== null, definition: writeBody(draft) };
	});
	const preview = createPreview<PreviewField, FieldPreview>(api.previewField);
	const previewForCurrent = $derived(
		previewRequest === null ? null : preview.answerFor(previewRequest)
	);

	$effect(() => {
		const request = previewRequest;
		if (missingName !== null || request === null) return;
		return preview.schedule(request);
	});

	// What stands in the way of Remove, as the button's explanation;
	// `null` once nothing does, or while the answer is not in.
	const blockedBy = $derived(usage.status === 'loaded' ? blockerPhrase(usage.summary) : null);
	const canRemove = $derived(
		existing !== null && !isId && usage.status !== 'loading' && blockedBy === null
	);

	onMount(() => {
		void schemaStore.load();
		if (existing !== null) void loadUsage(existing.name);
	});

	async function loadUsage(name: string): Promise<void> {
		const result = await api.getFieldUsage(name);
		usage =
			result.data !== undefined
				? { status: 'loaded', summary: summarizeUsage(result.data) }
				: { status: 'failed', message: result.error ?? 'could not load' };
	}

	async function save(): Promise<void> {
		saving = true;
		saveError = null;
		const body = writeBody(draft);
		const result =
			existing !== null
				? await api.updateField(existing.name, body)
				: await api.createField({ name: draft.name.trim(), definition: body });
		saving = false;
		if (result.error !== undefined) {
			saveError = result.error;
			return;
		}
		onsaved(result.diagnostics);
	}

	async function remove(dropValues: boolean): Promise<void> {
		if (existing === null) return;
		saving = true;
		saveError = null;
		const result = await api.deleteField(existing.name, { dropValues });
		saving = false;
		if (result.error !== undefined) {
			saveError = result.error;
			return;
		}
		onsaved(result.diagnostics);
	}

	function onTypeChange(event: Event & { currentTarget: HTMLSelectElement }): void {
		draft = retype(definition, draft, event.currentTarget.value as FieldType);
	}

	function onDefaultChange(next: DraftDefault | null): void {
		draft.default = next;
	}

	function onShapeChange(next: FieldShape): void {
		draft.shape = next;
	}

	function onResourceChange(next: string | null): void {
		draft.resource = next;
	}

	function onRollupChange(next: AggregateConfig | null): void {
		draft.aggregate = next;
	}

	function onValueKindChange(event: Event & { currentTarget: HTMLSelectElement }): void {
		const result = withValueKind(draft, event.currentTarget.value as ValueKind, {
			whenAllowed,
			computeAllowed,
			pullStart,
			choices
		});
		draft = result.draft;
		dropped = result.dropped;
	}

	function onPullChange(next: PullConfig): void {
		draft.recipe = { kind: 'pull', ...next };
	}

	function onComputeChange(next: ComputeRecipe): void {
		draft.recipe = next;
	}

	function onWhenChange(next: WhenRecipe): void {
		draft.recipe = next;
	}

	// The remove confirmation: one answer, or two once items hold a
	// value — remove the field and keep them, or drop them as well.
	const removeText = $derived(
		removeDialogText(usage.status === 'loaded' ? usage.summary.itemCount : null)
	);
</script>

<DraftPanel
	label={existing === null ? 'New field' : 'Field editor'}
	title={existing === null ? 'New field' : 'Edit field'}
	missing={missingName === null ? null : { noun: 'field', name: missingName }}
	{saveError}
	{saving}
	saveDisabled={!nameValid || shapeIssue !== null || previewForCurrent?.problem != null}
	saveTitle={shapeIssue ?? previewForCurrent?.problem ?? null}
	saveLabel={existing === null ? (saving ? 'Adding…' : 'Add field') : saving ? 'Saving…' : 'Save'}
	showRemove={existing !== null && !isId}
	removeDisabled={!canRemove}
	removeTitle={blockedBy === null ? null : `Remove the field from the ${blockedBy} first`}
	bind:removeButton
	preview={preview.state}
	previewPlaceholder={previewRequest === null ? 'Enter a name to see the entry.' : null}
	onsubmit={() => void save()}
	{onclose}
	onremove={() => (confirmingRemove = true)}
>
	{#snippet main()}
		<!-- ── Header block ─────────────────────────────────── -->

		<label class="row">
			<span class="label">Name</span>
			{#if existing === null}
				<input type="text" bind:value={draft.name} disabled={saving} placeholder="kebab_case" />
			{:else}
				<code class="locked-name">{existing.name}</code>
			{/if}
		</label>

		<label class="row">
			<span class="label">Type</span>
			<select
				value={draft.fieldType}
				disabled={saving || typeLocked}
				title={typeLocked ? 'Change the type in schema.yaml' : undefined}
				onchange={onTypeChange}
			>
				{#each fieldTypes as fieldType (fieldType)}
					<option value={fieldType}>{fieldType}</option>
				{/each}
			</select>
			{#if typeLocked && !isId}
				<span class="hint">Change the type in schema.yaml.</span>
			{/if}
		</label>

		<label class="row inline">
			<input type="checkbox" bind:checked={draft.required} disabled={saving} />
			<span class="label">Required</span>
		</label>

		<label class="row">
			<span class="label">Description</span>
			<textarea rows="3" bind:value={draft.description} disabled={saving}></textarea>
		</label>

		<div class="row">
			<span class="label">{valueKind === 'when' ? 'Default when no branch holds' : 'Default'}</span>
			{#if valueKind === 'pull' || valueKind === 'compute'}
				<span class="hint">
					None: a {valueKind === 'pull' ? 'pulled' : 'computed'} field has no default.{dropped !==
					null
						? ' The one set before was dropped.'
						: ''}
				</span>
			{:else}
				<!-- Beside conditions the default is the evaluated fallback, a
				     literal only: the generator choice is withheld. -->
				<DefaultControl
					value={draft.default}
					{spec}
					generators={valueKind === 'when' ? [] : generators}
					items={schemaStore.items}
					palette={schemaStore.palette}
					resourceOptions={schemaStore.optionsOfResource(draft.resource)}
					disabled={saving}
					onchange={onDefaultChange}
				/>
				{#if valueKind === 'when' && dropped === 'default'}
					<span class="hint">The generated default was dropped: the fallback is a fixed value.</span
					>
				{:else if valueKind === 'hand' && dropped === 'fallback'}
					<span class="hint">
						The fallback of the conditions was dropped: it would have become a default stamped on
						new items.
					</span>
				{/if}
			{/if}
		</div>

		<!-- ── Type-specific block ──────────────────────────────── -->

		<ShapeBlock
			shape={draft.shape}
			fieldType={draft.fieldType}
			problem={shapeIssue}
			disabled={saving}
			onchange={onShapeChange}
		/>

		{#if showsResource}
			<ResourceControl
				value={draft.resource}
				names={schemaStore.resourceNames}
				disabled={saving}
				onchange={onResourceChange}
			/>
		{/if}

		<!-- ── Value ────────────────────────────────────────────── -->

		{#if showsValue}
			<fieldset class="value" aria-label="Value">
				<label class="row">
					<span class="label">Value</span>
					<select value={valueKind} disabled={saving} onchange={onValueKindChange}>
						<option value="hand">Set by hand</option>
						<option
							value="when"
							disabled={!whenAllowed && valueKind !== 'when'}
							title={whenAllowed ? undefined : 'This type cannot be conditional'}
						>
							Conditional: by the first condition that holds
						</option>
						<option
							value="compute"
							disabled={!computeAllowed && valueKind !== 'compute'}
							title={computeAllowed ? undefined : 'This type cannot be computed'}
						>
							Computed by a formula
						</option>
						<option
							value="pull"
							disabled={pullStart === null && valueKind !== 'pull'}
							title={pullStart === null
								? 'Needs a link or links field that forbids cycles and a field that reduces into this type'
								: undefined}
						>
							Pulled from linked items
						</option>
					</select>
				</label>
				{#if draft.recipe?.kind === 'when'}
					<WhenBlock
						value={draft.recipe}
						{spec}
						tables={definition}
						{choices}
						operators={definition.comparison_operators}
						fields={definition.fields}
						constants={schemaStore.constants}
						items={schemaStore.items}
						palette={schemaStore.palette}
						problem={previewForCurrent?.problem ?? null}
						disabled={saving}
						onchange={onWhenChange}
					/>
				{:else if draft.recipe?.kind === 'pull'}
					<PullBlock
						value={draft.recipe}
						relations={pullRelationNames}
						sources={pullSourceFields}
						disabled={saving}
						onchange={onPullChange}
					/>
				{:else if draft.recipe?.kind === 'compute'}
					<ComputeBlock
						value={draft.recipe}
						fieldType={draft.fieldType}
						{references}
						problem={previewForCurrent?.problem ?? null}
						problemColumn={previewForCurrent?.problem_column ?? null}
						disabled={saving}
						onchange={onComputeChange}
					/>
				{:else}
					<span class="hint">
						Written on each item, or derived: chosen by conditions, computed by a formula over the
						item's own fields, or pulled from the items a relation points at.
					</span>
				{/if}
			</fieldset>
		{/if}

		<!-- ── Rollup ───────────────────────────────────────────── -->

		{#if showsRollup}
			<AggregateBlock
				value={draft.aggregate}
				functions={rollupFunctions}
				relations={rollupRelationNames}
				starting={rollupStart}
				disabled={saving}
				onchange={onRollupChange}
			/>
		{/if}
	{/snippet}

	{#snippet side()}
		{#if existing !== null}
			<UsageSection {usage} {blockedBy} />
		{/if}
	{/snippet}
</DraftPanel>

{#if confirmingRemove && removeButton !== undefined && existing !== null}
	<ConfirmDialog
		anchor={removeButton}
		title="Remove field '{existing.name}'?"
		body={removeText.body}
		confirmLabel={removeText.keepLabel}
		secondary={removeText.dropLabel === null
			? undefined
			: {
					label: removeText.dropLabel,
					onconfirm: () => {
						confirmingRemove = false;
						void remove(true);
					}
				}}
		destructive
		onconfirm={() => {
			confirmingRemove = false;
			void remove(false);
		}}
		oncancel={() => (confirmingRemove = false)}
	/>
{/if}

<style>
	.row {
		display: flex;
		flex-direction: column;
		gap: var(--space-1);
	}

	.row.inline {
		flex-direction: row;
		align-items: center;
		gap: var(--space-2);
	}

	.label {
		font-size: var(--text-sm);
		font-weight: 600;
		color: var(--color-fg-muted);
	}

	input[type='text'],
	select,
	textarea {
		width: 100%;
		padding: 0.25rem var(--space-2);
		background-color: var(--color-bg);
		color: var(--color-fg);
		border: 1px solid var(--color-border);
		border-radius: var(--radius-sm);
		font-size: var(--text-sm);
		font-family: inherit;
	}

	textarea {
		resize: vertical;
	}

	.value {
		display: flex;
		flex-direction: column;
		gap: var(--space-2);
		margin: 0;
		padding: 0;
		border: none;
	}

	.locked-name {
		align-self: flex-start;
		padding: 0.25rem var(--space-2);
		font-size: var(--text-sm);
	}

	.hint {
		font-size: var(--text-sm);
		color: var(--color-fg-muted);
		margin: 0;
	}
</style>
