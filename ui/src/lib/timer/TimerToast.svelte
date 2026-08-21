<!--
  The application-level toast — one slot, mounted in the root layout
  because a stop can happen from any page. It reports what the stop did
  (the amount added, the value before and after) and offers to take it
  back; a stop that wrote nothing still gets a toast saying so, because
  silence after pressing stop reads as breakage. It stays until
  dismissed or until the next timer action — an undo that expires after
  a few seconds is hostile to exactly the case it exists for.

  Undo reverts the write: the effort returns to exactly the
  before-value, including becoming absent again. Corrections beyond
  undo need no machinery here — the effort field is editable in the
  item form like any other field.
-->
<script lang="ts">
	import { timerStore } from '$lib/stores/timer.svelte';
	import { formatDurationSeconds } from '$lib/views/format';
	import { prettifyId } from '$lib/views/prettify';

	const toast = $derived(timerStore.toast);
</script>

{#if toast !== null}
	<div class="toast" role="status">
		<div class="content">
			{#if toast.kind === 'stopped'}
				{#if toast.result.write !== null}
					<p>
						Added {formatDurationSeconds(toast.result.write.added_seconds)} to
						{toast.result.field} on
						<a href={`/items/${encodeURIComponent(toast.result.item_id)}`}>
							{prettifyId(toast.result.item_id)}</a
						>
						— {toast.result.write.previous_seconds === null
							? 'none'
							: formatDurationSeconds(toast.result.write.previous_seconds)} →
						{formatDurationSeconds(toast.result.write.new_seconds)}.
					</p>
					{#if toast.result.write.mutation_caused_warning}
						<p class="muted">This write introduced a validation warning.</p>
					{/if}
					{#each toast.result.write.info_messages as message (message)}
						<p class="muted">{message}</p>
					{/each}
					<button type="button" disabled={timerStore.busy} onclick={() => void timerStore.undo()}>
						Undo
					</button>
				{:else}
					<p>
						Timer stopped after {formatDurationSeconds(toast.result.elapsed_seconds)} — under half a minute,
						nothing was written.
					</p>
				{/if}
			{:else if toast.kind === 'stop_failed'}
				<p class="error">Stop failed: {toast.message}</p>
				<p class="muted">The timer is still running — stop again after fixing the cause.</p>
			{:else if toast.kind === 'undone'}
				<p>
					Undone — {toast.result.field} on {prettifyId(toast.result.item_id)} is back to
					{toast.result.write !== null && toast.result.write.previous_seconds !== null
						? formatDurationSeconds(toast.result.write.previous_seconds)
						: 'unset'}.
				</p>
			{:else}
				<p class="error">Undo failed: {toast.message}</p>
				<button type="button" disabled={timerStore.busy} onclick={() => void timerStore.undo()}>
					Retry undo
				</button>
			{/if}
		</div>
		<button
			type="button"
			class="dismiss"
			aria-label="Dismiss"
			onclick={() => {
				timerStore.dismissToast();
			}}
		>
			×
		</button>
	</div>
{/if}

<style>
	.toast {
		position: fixed;
		bottom: var(--space-4);
		right: var(--space-4);
		z-index: 30;
		display: flex;
		align-items: flex-start;
		gap: var(--space-2);
		max-width: 24rem;
		background: var(--color-card);
		border: 1px solid var(--color-border);
		border-radius: var(--radius-md);
		box-shadow: var(--shadow-sm);
		padding: var(--space-3);
	}

	.content {
		display: flex;
		flex-direction: column;
		gap: var(--space-1);
		font-size: var(--text-sm);
	}

	.content p {
		margin: 0;
	}

	.content a {
		color: inherit;
	}

	.content button {
		align-self: flex-start;
		background: none;
		border: none;
		padding: 0;
		font-size: var(--text-sm);
		color: var(--color-fg-muted);
		text-decoration: underline;
		cursor: pointer;
	}

	.content button:hover:not(:disabled) {
		color: var(--color-fg);
	}

	.content button:disabled {
		opacity: 0.5;
		cursor: default;
	}

	.muted {
		color: var(--color-fg-muted);
	}

	.error {
		color: var(--color-error-fg);
	}

	.dismiss {
		background: none;
		border: none;
		padding: 0;
		color: var(--color-fg-muted);
		font-size: var(--text-lg);
		line-height: 1;
		cursor: pointer;
	}

	.dismiss:hover {
		color: var(--color-fg);
	}
</style>
