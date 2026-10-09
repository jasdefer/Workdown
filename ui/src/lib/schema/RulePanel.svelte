<!--
  The rule editor: the docked panel the schema page opens on a rule
  entry (`?rule=<name>`) or on Add rule (`?add-rule`). A header block —
  name, severity, description — then the match rows, the require rows
  and the count, and beside them the YAML the save would write, as the
  server renders it from the rows. The footer has Save, Cancel and
  Remove.

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
	import type { SchemaDefinitionData } from '$lib/api/generated/SchemaDefinitionData';
	import type { Severity } from '$lib/api/generated/Severity';
	import { schemaStore } from '$lib/stores/schema.svelte';
	import ConfirmDialog from '$lib/ui/ConfirmDialog.svelte';
	import SlideOver from '$lib/ui/SlideOver.svelte';
	import MatchRows from './MatchRows.svelte';
	import RequireRows from './RequireRows.svelte';
	import {
		anchorsOf,
		initialRuleDraft,
		newRuleDraft,
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
	const takenNames = $derived(definition.rules.map((rule) => rule.name));
	// What the browser can say is wrong, greying out Save with the reason.
	const problem = $derived(ruleProblem(draft, takenNames, existing?.name ?? null));

	// The server's rendering of the draft: the YAML and its verdict. Keyed
	// by the body it answered, so an answer to an older draft is dropped
	// and the Save guard only trusts an answer to the current one.
	type Preview =
		| { status: 'idle' }
		| { status: 'failed'; message: string }
		| { status: 'loaded'; key: string; yaml: string; problem: string | null };
	let preview = $state<Preview>({ status: 'idle' });
	let previewTimer: ReturnType<typeof setTimeout> | undefined;
	let latestKey = '';

	const editable = missingName === null && unrepresentable === null;
	const currentKey = $derived(JSON.stringify(ruleWriteBody(draft)));
	const previewForCurrent = $derived(
		preview.status === 'loaded' && preview.key === currentKey ? preview : null
	);

	$effect(() => {
		const key = currentKey;
		if (!editable) return;
		latestKey = key;
		clearTimeout(previewTimer);
		previewTimer = setTimeout(() => {
			void loadPreview(key, JSON.parse(key) as RuleDefinitionWrite);
		}, 300);
		return () => {
			clearTimeout(previewTimer);
		};
	});

	async function loadPreview(key: string, body: RuleDefinitionWrite): Promise<void> {
		const result = await api.previewRule({ replacing: existing?.name ?? null, definition: body });
		if (key !== latestKey) return;
		if (result.data === undefined) {
			preview = { status: 'failed', message: result.error ?? 'The preview could not be loaded.' };
			return;
		}
		preview = { status: 'loaded', key, yaml: result.data.yaml, problem: result.data.problem };
	}

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
		draft.shape = { ...draft.shape, count: { ...draft.shape.count, [bound]: boundOf(text) } };
	}

	function boundOf(text: string): number | null {
		if (text.trim() === '') return null;
		const parsed = Number.parseInt(text, 10);
		return Number.isNaN(parsed) || parsed < 0 ? null : parsed;
	}
</script>

<SlideOver
	label={existing === null ? 'New rule' : 'Rule editor'}
	size="wide"
	placement="docked"
	{onclose}
>
	{#snippet header()}
		<span class="panel-title">{existing === null ? 'New rule' : 'Edit rule'}</span>
	{/snippet}

	{#if missingName !== null}
		<p class="error" role="alert">There is no rule named <code>{missingName}</code>.</p>
		<div class="actions">
			<button type="button" class="cancel" onclick={onclose}>Close</button>
		</div>
	{:else if existing !== null && unrepresentable !== null}
		<div class="rule-form">
			{#if saveError !== null}
				<p class="error" role="alert">{saveError}</p>
			{/if}
			<div class="row">
				<span class="label">Name</span>
				<code class="locked-name">{existing.name}</code>
			</div>
			<p class="hint warning">
				This rule cannot be edited here: {unrepresentable}. Edit it in schema.yaml.
			</p>
			<pre class="yaml">{existing.body}</pre>
			<div class="actions">
				<button type="button" class="cancel" onclick={onclose}>Cancel</button>
				<button
					type="button"
					class="remove"
					bind:this={removeButton}
					disabled={saving}
					onclick={() => (confirmingRemove = true)}
				>
					Remove
				</button>
			</div>
		</div>
	{:else}
		<form
			class="rule-form"
			onsubmit={(event) => {
				event.preventDefault();
				void save();
			}}
		>
			{#if saveError !== null}
				<p class="error" role="alert">{saveError}</p>
			{/if}

			<!-- Two columns when the panel is wide enough: what is edited
			     on the left, the YAML it becomes on the right. -->
			<div class="columns">
				<div class="main">
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
				</div>

				<aside class="side">
					<!-- ── YAML ─────────────────────────────────────────── -->

					<section class="preview" aria-label="YAML">
						<div class="section-head">
							<span class="label">In schema.yaml</span>
							<span class="hint">As the save writes it.</span>
						</div>
						{#if preview.status === 'failed'}
							<span class="hint warning">Could not preview: {preview.message}</span>
						{:else if preview.status === 'loaded'}
							{#if preview.yaml !== ''}
								<pre class="yaml">{preview.yaml}</pre>
							{/if}
							{#if preview.problem !== null}
								<span class="hint warning" role="alert">{preview.problem}</span>
							{/if}
						{:else}
							<span class="hint">…</span>
						{/if}
					</section>
				</aside>
			</div>

			<!-- ── Footer ───────────────────────────────────────────── -->

			{#if problem !== null}
				<span class="hint">{problem}</span>
			{/if}
			<div class="actions">
				<button
					type="submit"
					class="primary"
					disabled={saving || problem !== null || previewForCurrent?.problem != null}
					title={problem ?? previewForCurrent?.problem ?? undefined}
				>
					{#if existing === null}
						{saving ? 'Adding…' : 'Add rule'}
					{:else}
						{saving ? 'Saving…' : 'Save'}
					{/if}
				</button>
				<button type="button" class="cancel" onclick={onclose}>Cancel</button>
				{#if existing !== null}
					<button
						type="button"
						class="remove"
						bind:this={removeButton}
						disabled={saving}
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
	.panel-title {
		font-weight: 600;
		color: var(--color-fg);
	}

	.rule-form {
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

	.section-head {
		display: flex;
		align-items: baseline;
		justify-content: space-between;
		gap: var(--space-2);
	}

	.preview {
		display: flex;
		flex-direction: column;
		gap: var(--space-1);
		padding: var(--space-3);
		border: 1px solid var(--color-border);
		border-radius: var(--radius-md);
		background-color: var(--color-surface);
	}

	.yaml {
		margin: 0;
		padding: var(--space-2);
		border-radius: var(--radius-sm);
		background-color: var(--color-bg);
		font-size: var(--text-sm);
		overflow-x: auto;
		white-space: pre;
	}

	.columns {
		display: grid;
		grid-template-columns: 1fr;
		gap: var(--space-4);
	}

	/* The panel body is the container; from 40rem on, the rows and the
	   YAML sit side by side, the same break as the field panel's. */
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

	/* Beside Cancel behind a thin rule, as the field panel places it. */
	.remove {
		margin-left: var(--space-1);
		padding-left: var(--space-4);
		border-left: 1px solid var(--color-border);
		border-radius: 0;
		border-top: none;
		border-right: none;
		border-bottom: none;
		background: none;
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
