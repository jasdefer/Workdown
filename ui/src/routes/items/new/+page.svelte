<!--
  Create-item form. Schema-driven: one `FieldEditor` per field, reusing
  the same type-dispatched editors as the detail panel. Here their
  `oncommit` records into a local `draft` instead of persisting — nothing
  exists to mutate yet — and the whole draft is POSTed on submit.

  Identity follows the server's rule: the new id is slugged from `title`
  unless a custom id is given in the optional override. A successful
  create with no warnings navigates to the new item; a save-with-warning
  create surfaces the diagnostics and offers a link rather than hiding
  them behind a redirect.
-->
<script lang="ts">
	import { goto } from '$app/navigation';
	import { api } from '$lib/api/client';
	import type { Diagnostic } from '$lib/api/generated/Diagnostic';
	import type { FieldMutation } from '$lib/api/generated/FieldMutation';
	import type { FieldValue } from '$lib/api/generated/FieldValue';
	import FieldEditor from '$lib/items/FieldEditor.svelte';
	import { schemaStore } from '$lib/stores/schema.svelte';
	import DiagnosticList from '$lib/ui/DiagnosticList.svelte';

	let draft = $state<Record<string, unknown>>({});
	let explicitId = $state('');
	let submitting = $state(false);
	let actionError = $state<string | null>(null);
	let warnings = $state<Diagnostic[]>([]);
	let created = $state<string | null>(null);

	$effect(() => {
		void schemaStore.load();
	});

	// Every field except the identity, which comes from the override or a
	// slug of `title`.
	const formFields = $derived(schemaStore.fields.filter((field) => field.name !== 'id'));

	function draftValue(name: string): FieldValue | null {
		return (draft[name] ?? null) as FieldValue | null;
	}

	function applyToDraft(name: string, mutation: FieldMutation): void {
		// FieldEditor only ever emits replace/unset. Unset clears the draft
		// entry (left undefined → omitted from the request).
		if (mutation.op === 'replace') {
			draft[name] = mutation.value;
		} else if (mutation.op === 'unset') {
			draft[name] = undefined;
		}
	}

	async function submit(): Promise<void> {
		submitting = true;
		actionError = null;
		warnings = [];
		created = null;

		const fields: Record<string, unknown> = { ...draft };
		const id = explicitId.trim();
		if (id !== '') fields.id = id;

		const result = await api.createItem({ fields, template: null });
		submitting = false;

		if (result.data === undefined) {
			actionError = result.error ?? 'Failed to create item.';
			return;
		}

		// The item index changed — refresh it for link pickers elsewhere.
		void schemaStore.reload();

		if (result.diagnostics.length > 0) {
			created = result.data.id;
			warnings = result.diagnostics;
			return;
		}
		void goto(`/items/${encodeURIComponent(result.data.id)}`);
	}
</script>

<section class="create">
	<div class="content">
		<h1>New item</h1>

		{#if actionError}
			<p class="error" role="alert">{actionError}</p>
		{/if}

		{#if created}
			<div class="created" role="status">
				<p>Created <code>{created}</code> with warnings:</p>
				<DiagnosticList diagnostics={warnings} />
				<a href="/items/{created}">Open {created} →</a>
			</div>
		{:else}
			<form
				onsubmit={(event) => {
					event.preventDefault();
					void submit();
				}}
			>
				<dl class="fields card">
					{#each formFields as field (field.name)}
						<dt>
							<span
								class="field-name"
								class:has-help={field.description !== null}
								title={field.description}>{field.name}</span
							>
							{#if field.required}<span class="req" title="required">*</span>{/if}
						</dt>
						<dd>
							<FieldEditor
								{field}
								value={draftValue(field.name)}
								items={schemaStore.items}
								palette={schemaStore.palette}
								disabled={submitting}
								oncommit={(mutation: FieldMutation) => {
									applyToDraft(field.name, mutation);
								}}
							/>
						</dd>
					{/each}

					<dt><span class="field-name">id</span></dt>
					<dd>
						<input
							type="text"
							placeholder="optional — defaults to a slug of the title"
							bind:value={explicitId}
							disabled={submitting}
						/>
					</dd>
				</dl>

				<div class="actions">
					<a href="/" class="cancel">Cancel</a>
					<button type="submit" disabled={submitting}>
						{submitting ? 'Creating…' : 'Create item'}
					</button>
				</div>
			</form>
		{/if}
	</div>
</section>

<style>
	.create {
		flex: 1;
		min-height: 0;
		overflow-y: auto;
		width: 100%;
		background: var(--color-canvas);
	}

	.content {
		max-width: 42rem;
		margin: 0 auto;
		padding: var(--space-2) 0;
		display: flex;
		flex-direction: column;
		gap: var(--space-3);
	}

	.card {
		background: var(--card-bg, var(--color-card));
		border: 1px solid var(--color-border);
		border-radius: var(--radius-md);
		box-shadow: var(--shadow-sm);
		padding: var(--space-4);
	}

	h1 {
		font-size: var(--text-lg);
		font-weight: 600;
		margin: 0;
	}

	.fields {
		display: grid;
		grid-template-columns: minmax(6rem, 9rem) 1fr;
		gap: var(--space-2) var(--space-3);
		margin: 0;
		align-items: start;
	}

	dt {
		display: flex;
		align-items: baseline;
		gap: 0.25rem;
		font-size: var(--text-sm);
		color: var(--color-fg-muted);
		padding-top: 0.3rem;
	}

	dd {
		margin: 0;
	}

	dd input {
		width: 100%;
		padding: 0.25rem var(--space-2);
		background-color: var(--color-bg);
		color: var(--color-fg);
		border: 1px solid var(--color-border);
		border-radius: var(--radius-sm);
		font-size: var(--text-sm);
	}

	.field-name.has-help {
		text-decoration: underline dotted;
		text-underline-offset: 2px;
		cursor: help;
	}

	.req {
		color: var(--color-error-fg);
	}

	.actions {
		display: flex;
		justify-content: flex-end;
		gap: var(--space-3);
		margin-top: var(--space-4);
		align-items: center;
	}

	button[type='submit'] {
		padding: 0.35rem var(--space-4);
		background-color: var(--color-accent, var(--color-fg));
		color: var(--color-bg);
		border: none;
		border-radius: var(--radius-sm);
		font-size: var(--text-sm);
		cursor: pointer;
	}

	button[type='submit']:disabled {
		opacity: 0.6;
		cursor: default;
	}

	.cancel {
		color: var(--color-fg-muted);
		font-size: var(--text-sm);
	}

	.error {
		color: var(--color-error-fg);
		font-size: var(--text-sm);
	}

	.created {
		display: flex;
		flex-direction: column;
		gap: var(--space-2);
		font-size: var(--text-sm);
	}
</style>
