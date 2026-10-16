<!--
  The shell the schema editor's panels share: a docked slide-over with
  a form in two columns — what is edited on the left (`main`), the YAML
  the save would write and anything else only shown on the right
  (`side`) — and a footer with Save, Cancel and Remove. The field and
  the rule panel differ in what they edit, not in how a draft is shown,
  previewed, saved or removed, so that part lives once, here.

  The YAML pane is the server's rendering of the draft and its verdict
  (`preview`, see `$lib/schema/preview.svelte`): the host decides when
  to ask, the shell only shows the answer. A `previewPlaceholder` takes
  the pane's place while there is nothing to preview yet. The shell
  never judges a draft itself: whether Save is enabled, and why not, is
  the host's call, handed down as `saveDisabled` and `saveTitle`.

  Owns no draft state. The Remove button is bound back to the host
  (`removeButton`) so its confirmation popover can anchor on it.
-->
<script lang="ts">
	import type { Snippet } from 'svelte';
	import type { PreviewAnswer, PreviewState } from '$lib/schema/preview.svelte';
	import SlideOver from './SlideOver.svelte';

	interface Props {
		/** The slide-over's accessible name. */
		label: string;
		/** The heading. */
		title: string;
		/**
		 * The entry the URL named but the schema does not have; the panel
		 * then shows only that, with a way out.
		 */
		missing: { noun: string; name: string } | null;
		/** The server's refusal of the last save, shown above the form. */
		saveError: string | null;
		saving: boolean;
		/** No Save: the entry is shown, not edited. */
		readOnly?: boolean;
		saveDisabled: boolean;
		/** Why Save is greyed out, as the button's tooltip. */
		saveTitle: string | null;
		saveLabel: string;
		showRemove: boolean;
		removeDisabled: boolean;
		/** Why Remove is greyed out, as the button's tooltip. */
		removeTitle: string | null;
		removeButton?: HTMLButtonElement | undefined;
		preview: PreviewState<PreviewAnswer>;
		/** Shown in the YAML pane instead of the preview while there is nothing to preview. */
		previewPlaceholder: string | null;
		/** The line beside the pane's title. */
		previewHint?: string;
		onsubmit: () => void;
		onclose: () => void;
		onremove: () => void;
		main: Snippet;
		side?: Snippet;
		/** Shown above the footer buttons: what the host has to say about the draft. */
		footerNote?: Snippet;
	}

	let {
		label,
		title,
		missing,
		saveError,
		saving,
		readOnly = false,
		saveDisabled,
		saveTitle,
		saveLabel,
		showRemove,
		removeDisabled,
		removeTitle,
		removeButton = $bindable(),
		preview,
		previewPlaceholder,
		previewHint = 'As the save writes it.',
		onsubmit,
		onclose,
		onremove,
		main,
		side,
		footerNote
	}: Props = $props();
</script>

<SlideOver {label} size="wide" placement="docked" {onclose}>
	{#snippet header()}
		<span class="panel-title">{title}</span>
	{/snippet}

	{#if missing !== null}
		<p class="error" role="alert">There is no {missing.noun} named <code>{missing.name}</code>.</p>
		<div class="actions">
			<button type="button" class="cancel" onclick={onclose}>Close</button>
		</div>
	{:else}
		<form
			class="draft-form"
			onsubmit={(event) => {
				event.preventDefault();
				if (!readOnly) onsubmit();
			}}
		>
			{#if saveError !== null}
				<p class="error" role="alert">{saveError}</p>
			{/if}

			<!-- Two columns when the panel is wide enough: what is edited
			     on the left, what is only shown on the right. One column
			     below that width. -->
			<div class="columns">
				<div class="main">
					{@render main()}
				</div>

				<aside class="side">
					<section class="preview" aria-label="YAML">
						<div class="section-head">
							<span class="label">In schema.yaml</span>
							<span class="hint">{previewHint}</span>
						</div>
						{#if previewPlaceholder !== null}
							<span class="hint">{previewPlaceholder}</span>
						{:else if preview.status === 'failed'}
							<span class="hint warning">Could not preview: {preview.message}</span>
						{:else if preview.status === 'loaded'}
							{#if preview.answer.yaml !== ''}
								<pre class="yaml">{preview.answer.yaml}</pre>
							{/if}
							{#if preview.answer.problem !== null}
								<span class="hint warning" role="alert">{preview.answer.problem}</span>
							{/if}
						{:else}
							<span class="hint">…</span>
						{/if}
					</section>
					{@render side?.()}
				</aside>
			</div>

			{@render footerNote?.()}
			<div class="actions">
				{#if !readOnly}
					<button
						type="submit"
						class="primary"
						disabled={saving || saveDisabled}
						title={saveTitle ?? undefined}
					>
						{saveLabel}
					</button>
				{/if}
				<button type="button" class="cancel" onclick={onclose}>Cancel</button>
				{#if showRemove}
					<button
						type="button"
						class="remove"
						bind:this={removeButton}
						disabled={saving || removeDisabled}
						title={removeTitle ?? undefined}
						onclick={onremove}
					>
						Remove
					</button>
				{/if}
			</div>
		</form>
	{/if}
</SlideOver>

<style>
	.panel-title {
		font-weight: 600;
		color: var(--color-fg);
	}

	.draft-form {
		display: flex;
		flex-direction: column;
		gap: var(--space-3);
	}

	.label {
		font-size: var(--text-sm);
		font-weight: 600;
		color: var(--color-fg-muted);
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
	   under the side column, far from the form. The confirmation
	   popover guards the click, so distance from Save buys nothing. */
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
