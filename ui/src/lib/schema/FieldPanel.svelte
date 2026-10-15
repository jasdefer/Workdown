<!--
  The field editor: the slide-over the schema page opens on a row click
  (`?field=<name>`) or on Add field (`?add`). A header block every type
  shares — name, type, required, description, default — then the
  type-specific block, the "Value" section with the way the value is
  filled (by hand, by the first condition that holds, by a formula, or
  pulled from linked items), the rollup section for the types that
  take one, and beside them the YAML the save would write, what the
  field is used by, and the footer with Save, Cancel and Remove.

  The YAML pane and the pre-save verdict come from the server, as in
  the rule panel: on every pause in editing the draft goes to the
  preview endpoint, which answers with the entry the write would
  produce and whether the schema would load with it and keep the
  field. A problem it reports greys out Save: the person is right
  there, so a draft the load would disable is not written and warned
  about afterwards, as an item would be (`schema-derived-field-editor`,
  decision 10).

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
	import type { FieldDefinitionWrite } from '$lib/api/generated/FieldDefinitionWrite';
	import type { FieldShape } from '$lib/api/generated/FieldShape';
	import type { FieldType } from '$lib/api/generated/FieldType';
	import type { PullConfig } from '$lib/api/generated/PullConfig';
	import type { SchemaDefinitionData } from '$lib/api/generated/SchemaDefinitionData';
	import { schemaStore } from '$lib/stores/schema.svelte';
	import ConfirmDialog from '$lib/ui/ConfirmDialog.svelte';
	import SlideOver from '$lib/ui/SlideOver.svelte';
	import AggregateBlock from './AggregateBlock.svelte';
	import ComputeBlock from './ComputeBlock.svelte';
	import DefaultControl from './DefaultControl.svelte';
	import PullBlock from './PullBlock.svelte';
	import ResourceControl from './ResourceControl.svelte';
	import ShapeBlock from './ShapeBlock.svelte';
	import WhenBlock from './WhenBlock.svelte';
	import {
		aggregateFunctionsFor,
		expressionReferences,
		generatorsFor,
		initialDraft,
		newCompute,
		newDraft,
		newPull,
		newRollup,
		newWhen,
		operandChoices,
		pullRelations,
		pullSources,
		retype,
		rollupRelations,
		shapeProblem,
		specOfDraft,
		takesAggregate,
		takesCompute,
		takesPull,
		takesResource,
		takesWhen,
		writeBody,
		type ComputeRecipe,
		type DraftDefault,
		type WhenRecipe
	} from './fieldDraft';
	import { blockerPhrase, removeDialogText, summarizeUsage, type UsageSummary } from './fieldUsage';
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

	type UsageState =
		| { status: 'loading' }
		| { status: 'failed'; message: string }
		| { status: 'loaded'; summary: UsageSummary };
	let usage = $state<UsageState>({ status: 'loading' });

	const isId = existing?.name === 'id';
	// Every existing field keeps its type here; a change is made in
	// `schema.yaml`, where any pair is possible and judged at load (the
	// write route's widening table stays as its guard). A field with a
	// recipe keeps it for good, the recipe was type-checked against it.
	const typeLocked = existing !== null;
	const invalidDefault = existing?.default?.kind === 'invalid' ? existing.default.reason : null;

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
	type ValueKind = 'hand' | 'pull' | 'compute' | 'when';
	const valueKind = $derived<ValueKind>(draft.recipe?.kind ?? 'hand');
	// Whether choosing "pulled" or "computed" dropped a default the draft
	// had (such a field has none), or choosing "conditional" dropped a
	// generated one (the fallback is a literal); and whether leaving
	// "conditional" dropped the fallback, which would otherwise turn
	// into an add-time default, a different meaning. The panel says what
	// happened either way (decision 7).
	let defaultDropped = $state(false);
	let fallbackDropped = $state(false);

	// The server's rendering of the draft: the YAML and its verdict. Keyed
	// by the request it answered, so an answer to an older draft is
	// dropped and the Save guard only trusts an answer to the current one.
	type Preview =
		| { status: 'idle' }
		| { status: 'failed'; message: string }
		| {
				status: 'loaded';
				key: string;
				yaml: string;
				problem: string | null;
				problemColumn: number | null;
		  };
	let preview = $state<Preview>({ status: 'idle' });
	let previewTimer: ReturnType<typeof setTimeout> | undefined;
	let latestKey = '';

	// A new field without a name has no entry to show yet; the pane says
	// so instead of relaying the server's refusal of the empty name.
	const previewRequest = $derived.by(() => {
		const name = existing?.name ?? draft.name.trim();
		if (name === '') return null;
		return { name, replacing: existing !== null, definition: writeBody(draft) };
	});
	const currentKey = $derived(previewRequest === null ? '' : JSON.stringify(previewRequest));
	const previewForCurrent = $derived(
		preview.status === 'loaded' && preview.key === currentKey ? preview : null
	);

	$effect(() => {
		const key = currentKey;
		if (missingName !== null || key === '') return;
		latestKey = key;
		clearTimeout(previewTimer);
		previewTimer = setTimeout(() => {
			void loadPreview(key);
		}, 300);
		return () => {
			clearTimeout(previewTimer);
		};
	});

	async function loadPreview(key: string): Promise<void> {
		const request = JSON.parse(key) as {
			name: string;
			replacing: boolean;
			definition: FieldDefinitionWrite;
		};
		const result = await api.previewField(request);
		if (key !== latestKey) return;
		if (result.data === undefined) {
			preview = { status: 'failed', message: result.error ?? 'The preview could not be loaded.' };
			return;
		}
		preview = {
			status: 'loaded',
			key,
			yaml: result.data.yaml,
			problem: result.data.problem,
			problemColumn: result.data.problem_column
		};
	}
	// What stands in the way of Remove, as the button's explanation;
	// `null` once nothing does, or while the answer is not in.
	const blockedBy = $derived(usage.status === 'loaded' ? blockerPhrase(usage.summary) : null);
	const canRemove = $derived(
		existing !== null && !isId && usage.status !== 'loading' && blockedBy === null
	);
	const nothingDependsOnIt = $derived(
		usage.status === 'loaded' && blockedBy === null && usage.summary.itemCount === 0
	);
	// The usage section's named groups, in the order they are shown;
	// an empty group is left out.
	const namedGroups = $derived.by(() => {
		if (usage.status !== 'loaded') return [];
		const summary = usage.summary;
		return [
			{ label: 'Rules', names: summary.rules },
			{ label: 'Views', names: summary.views },
			{ label: 'Config roles', names: summary.roles },
			{ label: 'Recipes', names: summary.recipes }
		].filter((group) => group.names.length > 0);
	});

	onMount(() => {
		void schemaStore.load();
		if (existing !== null) void loadUsage(existing.name);
	});

	async function loadUsage(name: string): Promise<void> {
		try {
			const result = await api.getFieldUsage(name);
			usage =
				result.data !== undefined
					? { status: 'loaded', summary: summarizeUsage(result.data) }
					: { status: 'failed', message: result.error ?? 'could not load' };
		} catch (error) {
			// The request itself failed, or the answer was not the shape
			// this page was built against (a server older than the page).
			// Say so rather than wait forever.
			usage = { status: 'failed', message: error instanceof Error ? error.message : String(error) };
		}
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
		const next = event.currentTarget.value as ValueKind;
		if (valueKind === 'when' && next !== 'when') {
			fallbackDropped = draft.default !== null;
			draft.default = null;
		}
		if (next === 'hand') {
			draft.recipe = null;
		} else if (next === 'when' && whenAllowed) {
			if (draft.default?.kind === 'generator') {
				defaultDropped = true;
				draft.default = null;
			}
			draft.recipe = newWhen(choices);
		} else if (next === 'pull' && pullStart !== null) {
			defaultDropped = draft.default !== null;
			draft.default = null;
			draft.recipe = { kind: 'pull', ...pullStart };
		} else if (next === 'compute' && computeAllowed) {
			defaultDropped = draft.default !== null;
			draft.default = null;
			draft.recipe = newCompute();
		}
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

<SlideOver
	label={existing === null ? 'New field' : 'Field editor'}
	size="wide"
	placement="docked"
	{onclose}
>
	{#snippet header()}
		<span class="panel-title">{existing === null ? 'New field' : 'Edit field'}</span>
	{/snippet}

	{#if missingName !== null}
		<p class="error" role="alert">There is no field named <code>{missingName}</code>.</p>
		<div class="actions">
			<button type="button" class="cancel" onclick={onclose}>Close</button>
		</div>
	{:else}
		<form
			class="field-form"
			onsubmit={(event) => {
				event.preventDefault();
				void save();
			}}
		>
			{#if saveError !== null}
				<p class="error" role="alert">{saveError}</p>
			{/if}

			<!-- Two columns when the panel is wide enough: what is edited
			     on the left, what is only shown — the recipe, the usage —
			     on the right. One column below that width. -->
			<div class="columns">
				<div class="main">
					<!-- ── Header block ─────────────────────────────────── -->

					<label class="row">
						<span class="label">Name</span>
						{#if existing === null}
							<input
								type="text"
								bind:value={draft.name}
								disabled={saving}
								placeholder="kebab_case"
							/>
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
						<span class="label"
							>{valueKind === 'when' ? 'Default when no branch holds' : 'Default'}</span
						>
						{#if valueKind === 'pull' || valueKind === 'compute'}
							<span class="hint">
								None: a {valueKind === 'pull' ? 'pulled' : 'computed'} field has no default.{defaultDropped
									? ' The one set before was dropped.'
									: ''}
							</span>
						{:else}
							<!-- Beside conditions the default is the evaluated fallback, a
							     literal only: the generator choice is withheld. -->
							<DefaultControl
								value={draft.default}
								spec={specOfDraft(draft)}
								generators={valueKind === 'when' ? [] : generators}
								items={schemaStore.items}
								palette={schemaStore.palette}
								resourceOptions={schemaStore.optionsOfResource(draft.resource)}
								disabled={saving}
								onchange={onDefaultChange}
							/>
							{#if valueKind === 'when' && defaultDropped}
								<span class="hint"
									>The generated default was dropped: the fallback is a fixed value.</span
								>
							{:else if valueKind === 'hand' && fallbackDropped}
								<span class="hint">
									The fallback of the conditions was dropped: it would have become a default stamped
									on new items.
								</span>
							{/if}
							{#if invalidDefault !== null}
								<span class="hint warning">The saved default is invalid: {invalidDefault}</span>
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
									spec={specOfDraft(draft)}
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
									problemColumn={previewForCurrent?.problemColumn ?? null}
									disabled={saving}
									onchange={onComputeChange}
								/>
							{:else}
								<span class="hint">
									Written on each item, or derived: chosen by conditions, computed by a formula over
									the item's own fields, or pulled from the items a relation points at.
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
				</div>

				<aside class="side">
					<!-- ── YAML ─────────────────────────────────────────────── -->

					<section class="preview" aria-label="YAML">
						<div class="section-head">
							<span class="label">In schema.yaml</span>
							<span class="hint">As the save writes it.</span>
						</div>
						{#if previewRequest === null}
							<span class="hint">Enter a name to see the entry.</span>
						{:else if preview.status === 'failed'}
							<span class="hint warning">Could not preview: {preview.message}</span>
						{:else if preview.status === 'loaded'}
							<pre class="yaml">{preview.yaml}</pre>
							{#if preview.problem !== null}
								<span class="hint warning" role="alert">{preview.problem}</span>
							{/if}
						{:else}
							<span class="hint">…</span>
						{/if}
					</section>

					<!-- ── Usage ────────────────────────────────────────────── -->

					{#if existing !== null}
						<section class="usage" aria-label="Used by">
							<span class="label">Used by</span>
							{#if usage.status === 'loading'}
								<span class="hint">Loading…</span>
							{:else if usage.status === 'failed'}
								<span class="hint">Could not load: {usage.message}</span>
							{:else if nothingDependsOnIt}
								<span class="hint">Nothing depends on this field.</span>
							{:else if usage.status === 'loaded'}
								{#each namedGroups as group (group.label)}
									<div class="group">
										<span class="group-label">{group.label}</span>
										<span class="group-names">
											{#each group.names as name, index (name)}{index > 0 ? ', ' : ''}<code
													>{name}</code
												>{/each}
										</span>
									</div>
								{/each}
								{#each usage.summary.other as message (message)}
									<p class="other">{message}</p>
								{/each}
								{#if usage.summary.itemCount > 0}
									<div class="group">
										<span class="group-label">Items</span>
										<span class="group-names">
											{usage.summary.itemCount === 1
												? '1 item holds a value'
												: `${String(usage.summary.itemCount)} items hold a value`}
										</span>
									</div>
								{/if}
								{#if blockedBy !== null}
									<span class="hint">Remove the field from the {blockedBy} first.</span>
								{/if}
							{/if}
						</section>
					{/if}
				</aside>
			</div>

			<!-- ── Footer ───────────────────────────────────────────── -->

			<div class="actions">
				<button
					type="submit"
					class="primary"
					disabled={saving ||
						!nameValid ||
						shapeIssue !== null ||
						previewForCurrent?.problem != null}
					title={shapeIssue ?? previewForCurrent?.problem ?? undefined}
				>
					{#if existing === null}
						{saving ? 'Adding…' : 'Add field'}
					{:else}
						{saving ? 'Saving…' : 'Save'}
					{/if}
				</button>
				<button type="button" class="cancel" onclick={onclose}>Cancel</button>
				{#if existing !== null && !isId}
					<button
						type="button"
						class="remove"
						bind:this={removeButton}
						disabled={saving || !canRemove}
						title={blockedBy !== null ? `Remove the field from the ${blockedBy} first` : undefined}
						onclick={() => (confirmingRemove = true)}
					>
						Remove
					</button>
				{/if}
			</div>
		</form>
	{/if}
</SlideOver>

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
	.panel-title {
		font-weight: 600;
		color: var(--color-fg);
	}

	.field-form {
		display: flex;
		flex-direction: column;
		gap: var(--space-3);
	}

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

	.hint.warning {
		color: var(--color-warning-fg);
	}

	.section-head {
		display: flex;
		align-items: baseline;
		justify-content: space-between;
		gap: var(--space-2);
	}

	.preview,
	.usage {
		display: flex;
		flex-direction: column;
		gap: var(--space-1);
		padding: var(--space-3);
		border: 1px solid var(--color-border);
		border-radius: var(--radius-md);
		background-color: var(--color-surface);
	}

	pre {
		margin: 0;
		padding: var(--space-2);
		border-radius: var(--radius-sm);
		background-color: var(--color-bg);
		font-size: var(--text-sm);
		overflow-x: auto;
	}

	.yaml {
		white-space: pre;
	}

	.columns {
		display: grid;
		grid-template-columns: 1fr;
		gap: var(--space-4);
	}

	/* The panel body is the container; from 40rem on, the edited block
	   and the shown blocks sit side by side. */
	@container (min-width: 40rem) {
		.columns {
			grid-template-columns: 3fr 2fr;
			align-items: start;
		}
	}

	.main,
	.side {
		display: flex;
		flex-direction: column;
		gap: var(--space-3);
		min-width: 0;
	}

	.group {
		display: grid;
		grid-template-columns: 6rem 1fr;
		gap: var(--space-2);
		font-size: var(--text-sm);
	}

	.group-label {
		color: var(--color-fg-muted);
	}

	.group-names {
		overflow-wrap: anywhere;
	}

	.group-names code {
		background-color: transparent;
		padding: 0;
	}

	/* A finding with no name to show: the message, wrapped, never cut. */
	.other {
		margin: 0;
		font-size: var(--text-sm);
		white-space: pre-wrap;
		overflow-wrap: anywhere;
		color: var(--color-fg-muted);
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
		background: none;
		border: none;
		color: var(--color-fg-muted);
		font-size: var(--text-sm);
		cursor: pointer;
		padding: 0;
	}

	/* Beside Cancel behind a thin rule, not at the panel's far edge:
	   the footer spans a wide panel, and a button pushed to its end sat
	   under the usage column, far from the form. The confirmation
	   popover guards the click, so distance from Save buys nothing. */
	.remove {
		margin-left: var(--space-1);
		padding-left: var(--space-4);
		border-left: 1px solid var(--color-border);
		border-radius: 0;
		border-top: none;
		border-right: none;
		border-bottom: none;
		color: var(--color-error-fg);
		font-size: var(--text-sm);
		padding-top: 0.35rem;
		padding-right: 0;
		padding-bottom: 0.35rem;
		cursor: pointer;
	}

	.remove:disabled {
		opacity: 0.5;
		cursor: not-allowed;
	}

	.error {
		color: var(--color-error-fg);
		background-color: var(--color-error-bg);
		padding: var(--space-2);
		border-radius: var(--radius-sm);
		font-size: var(--text-sm);
		margin: 0;
	}

	.error code {
		background-color: transparent;
		padding: 0;
	}
</style>
