<!--
  The rule editor: the docked panel the schema page opens on a rule
  entry (`?rule=<name>`) or on Add rule (`?add-rule`). A header block —
  name, severity, description — then the match rows, the require rows
  and the count, and beside them the YAML the save would write, as the
  server renders it from the rows. The shell — columns, YAML pane,
  footer — is `DraftPanel`, shared with the field editor.

  The panel copies the rule into a draft when it mounts and never reads
  the rule again, for the reason the field panel gives: the page behind
  it refetches on every file change. Switching to another rule remounts
  the panel (the page keys it by name).

  The YAML pane and the pre-save verdict come from the server: on every
  pause in editing the draft goes to the preview endpoint, which answers
  with the text the write would produce and whether the schema would
  still load with it. The pane is therefore never a second opinion on
  the grammar, and a refusal is shown before Save is clicked. What the
  browser judges itself (`ruleProblem`) is only what saves a round
  trip; the server's verdict wins where they differ.

  Saving writes directly. A rule the server refuses comes back as its
  message in the panel and nothing is written; a write that loads but
  warns is written, and the warnings go to the page through `onsaved`.
  A rule whose body the rows cannot carry — a quantifier nested in a
  quantifier — is shown as text with the reason, and only Remove is
  offered.
-->
<script lang="ts">
	import { onMount, untrack } from 'svelte';
	import { api } from '$lib/api/client';
	import type { Diagnostic } from '$lib/api/generated/Diagnostic';
	import type { MatchRow } from '$lib/api/generated/MatchRow';
	import type { RequireRow } from '$lib/api/generated/RequireRow';
	import type { RuleData } from '$lib/api/generated/RuleData';
	import type { RuleDefinitionWrite } from '$lib/api/generated/RuleDefinitionWrite';
	import type { RulePreview } from '$lib/api/generated/RulePreview';
	import type { SchemaDefinitionData } from '$lib/api/generated/SchemaDefinitionData';
	import type { Severity } from '$lib/api/generated/Severity';
	import { schemaStore } from '$lib/stores/schema.svelte';
	import ConfirmDialog from '$lib/ui/ConfirmDialog.svelte';
	import DraftPanel from '$lib/ui/DraftPanel.svelte';
	import MatchRows from './MatchRows.svelte';
	import RequireRows from './RequireRows.svelte';
	import { createPreview, type PreviewState } from './preview.svelte';
	import {
		anchorsOf,
		initialRuleDraft,
		newRuleDraft,
		parseCount,
		ruleProblem,
		ruleWriteBody
	} from './ruleDraft';
	import type { RulePanelTarget } from './schemaPage';

	interface Props {
		definition: SchemaDefinitionData;
		target: RulePanelTarget;
		onclose: () => void;
		/** The write happened; the host shows `warnings` and closes the panel. */
		onsaved: (warnings: Diagnostic[]) => void;
	}

	let { definition, target, onclose, onsaved }: Props = $props();

	// Read once, deliberately: the rule as served when the panel opened.
	const existing: RuleData | null = untrack(() =>
		target.kind === 'edit-rule'
			? (definition.rules.find((rule) => rule.name === target.name) ?? null)
			: null
	);
	const missingName = untrack(() =>
		target.kind === 'edit-rule' && existing === null ? target.name : null
	);
	const unrepresentable = existing?.shape.kind === 'unrepresentable' ? existing.shape.reason : null;

	let draft = $state(
		untrack(() =>
			existing !== null && existing.shape.kind === 'rows'
				? initialRuleDraft(existing, existing.shape.shape)
				: newRuleDraft()
		)
	);
	let saving = $state(false);
	let saveError = $state<string | null>(null);
	let confirmingRemove = $state(false);
	let removeButton = $state<HTMLButtonElement>();

	// Exhaustive by type: a severity added in Rust fails the build here
	// until it has a label.
	const SEVERITY_LABELS: Record<Severity, string> = {
		error: 'error — an item breaking it fails validation',
		warning: 'warning — an item breaking it only gets a warning'
	};
	const severities = Object.keys(SEVERITY_LABELS) as Severity[];

	const anchors = $derived(anchorsOf(definition.fields));
	// What is not filled in yet, greying out Save with the reason; the
	// server's verdict on a filled-in draft arrives through the preview.
	const problem = $derived(ruleProblem(draft));

	const editable = missingName === null && unrepresentable === null;
	const preview = createPreview<RuleDefinitionWrite, RulePreview>((body) =>
		api.previewRule({ replacing: existing?.name ?? null, definition: body })
	);
	const body = $derived(ruleWriteBody(draft));
	const previewForCurrent = $derived(preview.answerFor(body));

	$effect(() => {
		const request = body;
		if (!editable) return;
		return preview.schedule(request);
	});

	// A rule the rows cannot carry is shown as the file has it, in the
	// same pane the preview would use.
	const shownAsText = $derived<PreviewState<RulePreview>>(
		existing !== null && unrepresentable !== null
			? { status: 'loaded', key: '', answer: { yaml: existing.body, problem: null } }
			: preview.state
	);

	onMount(() => {
		void schemaStore.load();
	});

	async function save(): Promise<void> {
		saving = true;
		saveError = null;
		const body = ruleWriteBody(draft);
		const result =
			existing !== null ? await api.updateRule(existing.name, body) : await api.createRule(body);
		saving = false;
		if (result.error !== undefined) {
			saveError = result.error;
			return;
		}
		onsaved(result.diagnostics);
	}

	async function remove(): Promise<void> {
		if (existing === null) return;
		saving = true;
		saveError = null;
		const result = await api.deleteRule(existing.name);
		saving = false;
		if (result.error !== undefined) {
			saveError = result.error;
			return;
		}
		onsaved(result.diagnostics);
	}

	// One bound of the count from its input; blank clears it.
	function setCountBound(bound: 'min' | 'max', text: string): void {
		if (draft.shape.count === null) return;
		draft.shape = { ...draft.shape, count: { ...draft.shape.count, [bound]: parseCount(text) } };
	}
</script>

<DraftPanel
	label={existing === null ? 'New rule' : 'Rule editor'}
	title={existing === null ? 'New rule' : 'Edit rule'}
	missing={missingName === null ? null : { noun: 'rule', name: missingName }}
	{saveError}
	{saving}
	readOnly={unrepresentable !== null}
	saveDisabled={problem !== null || previewForCurrent?.problem != null}
	saveTitle={problem ?? previewForCurrent?.problem ?? null}
	saveLabel={existing === null ? (saving ? 'Adding…' : 'Add rule') : saving ? 'Saving…' : 'Save'}
	showRemove={existing !== null}
	removeDisabled={false}
	removeTitle={null}
	bind:removeButton
	preview={shownAsText}
	previewPlaceholder={null}
	previewHint={unrepresentable === null ? 'As the save writes it.' : 'As the file has it.'}
	onsubmit={() => void save()}
	{onclose}
	onremove={() => (confirmingRemove = true)}
>
	{#snippet main()}
		{#if existing !== null && unrepresentable !== null}
			<div class="row">
				<span class="label">Name</span>
				<code class="locked-name">{existing.name}</code>
			</div>
			<p class="hint warning">
				This rule cannot be edited here: {unrepresentable}. Edit it in schema.yaml.
			</p>
		{:else}
			<!-- ── Header block ─────────────────────────────────── -->

			<label class="row">
				<span class="label">Name</span>
				<input
					type="text"
					bind:value={draft.name}
					disabled={saving}
					placeholder="kebab-case"
					spellcheck="false"
				/>
			</label>

			<label class="row">
				<span class="label">Severity</span>
				<select bind:value={draft.severity} disabled={saving}>
					{#each severities as severity (severity)}
						<option value={severity}>{SEVERITY_LABELS[severity]}</option>
					{/each}
				</select>
			</label>

			<label class="row">
				<span class="label">Description</span>
				<textarea rows="2" bind:value={draft.description} disabled={saving}></textarea>
			</label>

			<!-- ── Rows ─────────────────────────────────────────── -->

			<MatchRows
				rows={draft.shape.match_rows}
				{anchors}
				fields={definition.fields}
				items={schemaStore.items}
				palette={schemaStore.palette}
				optionsOfResource={schemaStore.optionsOfResource}
				disabled={saving}
				onchange={(rows: MatchRow[]) => {
					draft.shape = { ...draft.shape, match_rows: rows };
				}}
			/>

			<RequireRows
				rows={draft.shape.require_rows}
				{anchors}
				fields={definition.fields}
				tables={definition}
				items={schemaStore.items}
				palette={schemaStore.palette}
				disabled={saving}
				onchange={(rows: RequireRow[]) => {
					draft.shape = { ...draft.shape, require_rows: rows };
				}}
			/>

			<!-- ── Count ────────────────────────────────────────── -->

			<fieldset class="count">
				<label class="row inline">
					<input
						type="checkbox"
						checked={draft.shape.count !== null}
						disabled={saving}
						onchange={(event: Event & { currentTarget: HTMLInputElement }) => {
							draft.shape = {
								...draft.shape,
								count: event.currentTarget.checked ? { min: null, max: null } : null
							};
						}}
					/>
					<span class="label">Limit how many items may match</span>
				</label>
				{#if draft.shape.count !== null}
					<div class="bounds">
						<label class="bound">
							<span class="hint">at least</span>
							<input
								type="number"
								min="0"
								step="1"
								value={draft.shape.count.min ?? ''}
								disabled={saving}
								onchange={(event: Event & { currentTarget: HTMLInputElement }) => {
									setCountBound('min', event.currentTarget.value);
								}}
							/>
						</label>
						<label class="bound">
							<span class="hint">at most</span>
							<input
								type="number"
								min="0"
								step="1"
								value={draft.shape.count.max ?? ''}
								disabled={saving}
								onchange={(event: Event & { currentTarget: HTMLInputElement }) => {
									setCountBound('max', event.currentTarget.value);
								}}
							/>
						</label>
					</div>
				{/if}
			</fieldset>
		{/if}
	{/snippet}

	{#snippet footerNote()}
		{#if unrepresentable === null && problem !== null}
			<span class="hint">{problem}</span>
		{/if}
	{/snippet}
</DraftPanel>

{#if confirmingRemove && removeButton !== undefined && existing !== null}
	<ConfirmDialog
		anchor={removeButton}
		title="Remove rule '{existing.name}'?"
		body="Items it flags today stop being flagged. Nothing else names a rule."
		confirmLabel="Remove"
		destructive
		onconfirm={() => {
			confirmingRemove = false;
			void remove();
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
	input[type='number'],
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

	.count {
		display: flex;
		flex-direction: column;
		gap: var(--space-2);
		margin: 0;
		padding: 0;
		border: none;
	}

	.bounds {
		display: flex;
		gap: var(--space-3);
	}

	.bound {
		display: flex;
		align-items: center;
		gap: var(--space-2);
	}

	.bound input {
		width: 5rem;
	}
</style>
