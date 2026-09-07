<!--
  The "Commit & push" dialog — the review moment before anything leaves
  the machine. Opens on a preview: the workdown files the commit would
  cover (items, then definition files, each with what happened to it),
  the files outside the workdown paths that are *not* included, and the
  generated commit message in an editable box. Confirming runs commit,
  pull-if-behind and push as one server action; the answer replaces the
  form with a three-row checklist (committed, pull, push), a stopped
  step carrying its sentence and git's raw output behind a toggle.

  A refusal (409) keeps the form: the message is shown, the preview is
  reloaded so the list is current — the set of changes may have moved
  under the dialog — and the typed message is kept.

  A centered native <dialog> opened with showModal(): top layer, inert
  page, Escape for free. State lives in the git store; wording in
  `commitDialog.ts` (unit-tested).
-->
<script lang="ts">
	import { tick } from 'svelte';
	import { gitStore } from '$lib/stores/git.svelte';
	import { pluralize } from '$lib/views/format';
	import { changeLabel, checklist, groupFiles, splitDetails } from './commitDialog';

	let dialog = $state<HTMLDialogElement>();
	let textarea = $state<HTMLTextAreaElement>();
	let message = $state('');
	// The generated message seeds the box once; a reload after a stale
	// refusal must not overwrite what the user typed.
	let seeded = false;

	const preview = $derived(gitStore.preview);
	const groups = $derived(preview === null ? null : groupFiles(preview.files));
	const result = $derived(gitStore.commitResult);
	const rows = $derived(result === null ? [] : checklist(result));
	const error = $derived(gitStore.commitError === null ? null : splitDetails(gitStore.commitError));
	const canConfirm = $derived(
		preview !== null && preview.files.length > 0 && message.trim().length > 0 && !gitStore.busy
	);

	$effect(() => {
		if (dialog === undefined || dialog.open) return;
		const element = dialog;
		element.showModal();
		return () => {
			element.close();
		};
	});

	$effect(() => {
		if (preview !== null && !seeded) {
			message = preview.message;
			seeded = true;
			void tick().then(() => {
				fitTextarea();
				textarea?.focus();
			});
		}
	});

	/** Size the message box to its content: a generated body with ten
	 * lines should be readable without scrolling inside the box, while
	 * the CSS `max-height` keeps a long one from swallowing the dialog. */
	function fitTextarea(): void {
		if (textarea === undefined) return;
		textarea.style.height = 'auto';
		textarea.style.height = `${String(textarea.scrollHeight + 2)}px`;
	}

	function close(): void {
		gitStore.closeCommitDialog();
	}

	function onclick(event: MouseEvent): void {
		// The box is fully covered by `.content`; a click on the dialog
		// element itself landed on the backdrop.
		if (event.target === dialog) close();
	}
</script>

<dialog bind:this={dialog} aria-labelledby="commit-dialog-title" onclose={close} {onclick}>
	<div class="content">
		<p class="title" id="commit-dialog-title">Commit &amp; push</p>

		{#if result !== null}
			<ul class="checklist">
				{#each rows as row (row.label)}
					<li class={row.state}>
						<span class="mark" aria-hidden="true">
							{row.state === 'done' ? '✓' : row.state === 'skipped' ? '–' : '✕'}
						</span>
						<span class="row-text">
							{row.label}
							{#if row.details !== null}
								<details>
									<summary>Details</summary>
									<pre>{row.details}</pre>
								</details>
							{/if}
						</span>
					</li>
				{/each}
			</ul>
			<div class="actions">
				<button type="button" class="confirm" onclick={close}>Close</button>
			</div>
		{:else}
			{#if gitStore.previewError !== null}
				<p class="error">{gitStore.previewError}</p>
			{:else if preview === null || groups === null}
				<p class="muted">Reading the changes…</p>
			{:else if preview.files.length === 0}
				<p class="muted">Nothing to commit — no workdown files have changed.</p>
			{:else}
				<div class="files">
					{#if groups.items.length > 0}
						<p class="group-title">Work items</p>
						<ul>
							{#each groups.items as file (file.path)}
								<li>
									<span class="path">{file.path}</span>
									<span class="change">{changeLabel(file.change)}</span>
								</li>
							{/each}
						</ul>
					{/if}
					{#if groups.definitions.length > 0}
						<p class="group-title">Definition files</p>
						<ul>
							{#each groups.definitions as file (file.path)}
								<li>
									<span class="path">{file.path}</span>
									<span class="change">{file.role} · {changeLabel(file.change)}</span>
								</li>
							{/each}
						</ul>
					{/if}
					{#if preview.outside.length > 0}
						<details class="outside" open={preview.outside.length <= 5}>
							<summary>
								Not included: {pluralize(preview.outside.length, 'file')} outside the workdown paths
							</summary>
							<ul>
								{#each preview.outside as path (path)}
									<li class="path">{path}</li>
								{/each}
							</ul>
							<p class="muted">
								These stay uncommitted. If the branch is behind, the pull step will stop over them.
							</p>
						</details>
					{/if}
				</div>
				<label class="message-label" for="commit-dialog-message">Commit message</label>
				<textarea
					id="commit-dialog-message"
					bind:this={textarea}
					bind:value={message}
					oninput={fitTextarea}
					rows="6"
					spellcheck="false"
				></textarea>
			{/if}

			{#if error !== null}
				<div class="error">
					<p>{error.sentence}</p>
					{#if error.details !== null}
						<details>
							<summary>Details</summary>
							<pre>{error.details}</pre>
						</details>
					{/if}
				</div>
			{/if}

			<div class="actions">
				<button type="button" class="cancel" onclick={close}>Cancel</button>
				<button
					type="button"
					class="cancel"
					onclick={() => void gitStore.reloadPreview()}
					disabled={gitStore.busy}
					title="Read the changes again"
				>
					Reload
				</button>
				<button
					type="button"
					class="confirm"
					disabled={!canConfirm}
					onclick={() => void gitStore.commit(message)}
				>
					{gitStore.busy ? 'Working…' : 'Commit & push'}
				</button>
			</div>
		{/if}
	</div>
</dialog>

<style>
	dialog {
		/* The global reset zeroes every margin, which takes away the user
		   agent's `margin: auto` that centers a modal dialog — so center
		   it explicitly. */
		position: fixed;
		inset: 0;
		margin: auto;
		width: min(44rem, calc(100vw - 2rem));
		max-height: calc(100vh - 2rem);
		padding: 0;
		background-color: var(--color-card);
		color: var(--color-fg);
		border: 1px solid var(--color-border);
		border-radius: var(--radius-md);
		box-shadow: var(--shadow-sm);
	}

	dialog::backdrop {
		background: rgb(0 0 0 / 0.3);
	}

	.content {
		padding: var(--space-4) var(--space-6);
		display: flex;
		flex-direction: column;
		gap: var(--space-3);
		max-height: calc(100vh - 2rem);
		overflow: auto;
	}

	/* The file lists scroll on their own past a screenful, so a big
	   batch never pushes the message box out of view. */
	.files > ul {
		max-height: 30vh;
		overflow: auto;
	}

	.title {
		margin: 0;
		font-size: var(--text-base);
		font-weight: 600;
	}

	.muted {
		margin: 0;
		color: var(--color-fg-muted);
		font-size: var(--text-sm);
	}

	.files ul,
	.checklist {
		list-style: none;
		margin: 0;
		padding: 0;
	}

	.group-title {
		margin: var(--space-2) 0 var(--space-1);
		font-size: var(--text-sm);
		font-weight: 600;
		color: var(--color-fg-muted);
	}

	.files li {
		display: flex;
		justify-content: space-between;
		gap: var(--space-3);
		font-size: var(--text-sm);
		padding: 0.15rem 0;
	}

	.path {
		font-family: var(--font-mono);
		overflow: hidden;
		text-overflow: ellipsis;
		white-space: nowrap;
	}

	.change {
		color: var(--color-fg-muted);
		white-space: nowrap;
	}

	.outside {
		margin: var(--space-2) 0 0;
		font-size: var(--text-sm);
	}

	.outside summary {
		color: var(--color-warning-fg);
	}

	.outside ul {
		margin-top: var(--space-1);
		max-height: 10rem;
		overflow: auto;
	}

	.outside li {
		padding: 0.1rem 0;
		font-size: var(--text-sm);
	}

	.outside .muted {
		margin-top: var(--space-1);
	}

	.message-label {
		font-size: var(--text-sm);
		font-weight: 600;
	}

	textarea {
		width: 100%;
		box-sizing: border-box;
		font: inherit;
		font-family: var(--font-mono);
		font-size: var(--text-sm);
		padding: var(--space-2);
		color: var(--color-fg);
		background-color: var(--color-bg);
		border: 1px solid var(--color-border);
		border-radius: var(--radius-sm);
		resize: vertical;
		line-height: 1.45;
		min-height: 7rem;
		max-height: 45vh;
		overflow-y: auto;
	}

	.checklist li {
		display: flex;
		gap: var(--space-2);
		font-size: var(--text-sm);
		padding: 0.2rem 0;
	}

	.mark {
		width: 1.2rem;
		flex: none;
		font-weight: 600;
	}

	.checklist li.done .mark {
		color: var(--color-accent);
	}

	.checklist li.skipped {
		color: var(--color-fg-muted);
	}

	.checklist li.stopped .mark {
		color: var(--color-error-fg);
	}

	.row-text {
		flex: 1;
		min-width: 0;
	}

	.error {
		margin: 0;
		padding: var(--space-2) var(--space-3);
		font-size: var(--text-sm);
		color: var(--color-error-fg);
		background-color: var(--color-error-bg);
		border-radius: var(--radius-sm);
	}

	.error p {
		margin: 0;
	}

	details {
		margin-top: var(--space-1);
	}

	summary {
		cursor: pointer;
		font-size: var(--text-sm);
		color: var(--color-fg-muted);
	}

	pre {
		margin: var(--space-1) 0 0;
		padding: var(--space-2);
		font-family: var(--font-mono);
		font-size: var(--text-sm);
		white-space: pre-wrap;
		word-break: break-word;
		background-color: var(--color-bg);
		border-radius: var(--radius-sm);
		max-height: 12rem;
		overflow: auto;
	}

	.actions {
		display: flex;
		justify-content: flex-end;
		gap: var(--space-3);
		margin-top: var(--space-2);
	}

	.cancel {
		background: none;
		border: none;
		padding: 0.35rem var(--space-2);
		color: var(--color-fg-muted);
		font-size: var(--text-sm);
		cursor: pointer;
	}

	.cancel:hover:enabled {
		color: var(--color-fg);
	}

	.confirm {
		background-color: var(--color-accent);
		color: var(--color-accent-fg);
		border: 1px solid var(--color-accent);
		border-radius: var(--radius-sm);
		padding: 0.35rem var(--space-4);
		font-size: var(--text-sm);
		font-weight: 600;
		cursor: pointer;
	}

	.confirm:disabled {
		opacity: 0.6;
		cursor: default;
	}
</style>
