<!--
  The git sync pill — the header's pull/push surface, shown only when
  the project opted in (`serve.git_controls: true`) and sits inside a
  git repository. Shows the branch and a glanceable summary
  (`↓behind ↑ahead · N local`, or `in sync`), a Pull button, and a Push
  button that is enabled only when local commits exist — uncommitted
  edits never leave the machine from here; the tooltip and the dirty
  hint say so.

  Display rules live in `gitPill.ts` (unit-tested); operations and
  state live in the git store.
-->
<script lang="ts">
	import { gitStore } from '$lib/stores/git.svelte';
	import { pillModel } from './gitPill';

	const model = $derived(pillModel(gitStore.status, gitStore.busy));

	// A commit made in a terminal changes nothing the file watcher sees
	// (only `.git/`), so no live-update ping arrives and the counts go
	// stale. But to click Pull or Push the user must focus this window
	// again — refresh at that moment, and the pill is current by the
	// time the cursor reaches the button.
	$effect(() => {
		const refresh = () => {
			void gitStore.refresh();
		};
		window.addEventListener('focus', refresh);
		document.addEventListener('visibilitychange', refresh);
		return () => {
			window.removeEventListener('focus', refresh);
			document.removeEventListener('visibilitychange', refresh);
		};
	});
</script>

{#if model.visible}
	<div class="git-pill" title={model.dirtyHint ?? undefined}>
		<span class="branch">{model.branch}</span>
		<span class="summary">{model.summary}</span>
		<button
			class="action"
			onclick={() => void gitStore.pull()}
			disabled={!model.canPull}
			title="Pull the latest changes from the remote"
		>
			Pull
		</button>
		<button
			class="action"
			onclick={() => void gitStore.push()}
			disabled={!model.canPush}
			title={model.pushTitle}
		>
			Push
		</button>
		{#if gitStore.message !== null}
			<button
				class="message {gitStore.message.kind}"
				title="Dismiss"
				onclick={() => {
					gitStore.dismissMessage();
				}}
			>
				{gitStore.message.text}
			</button>
		{/if}
	</div>
{/if}

<style>
	.git-pill {
		display: inline-flex;
		align-items: center;
		gap: 0.4rem;
		font-size: var(--text-sm);
		background: var(--color-card);
		border: 1px solid var(--color-border);
		border-radius: var(--radius-full);
		color: var(--color-fg);
		padding: 0.25rem 0.7rem;
	}

	.branch {
		font-weight: 600;
		max-width: 10rem;
		overflow: hidden;
		text-overflow: ellipsis;
		white-space: nowrap;
	}

	.summary {
		color: var(--color-fg-muted);
		font-variant-numeric: tabular-nums;
		white-space: nowrap;
	}

	.action {
		font: inherit;
		font-size: var(--text-sm);
		color: var(--color-fg);
		background: none;
		border: 1px solid var(--color-border);
		border-radius: var(--radius-full);
		padding: 0.05rem 0.55rem;
		cursor: pointer;
	}

	.action:hover:enabled {
		background: var(--color-surface);
	}

	.action:disabled {
		color: var(--color-fg-muted);
		cursor: default;
		opacity: 0.6;
	}

	/* The result message doubles as its own dismiss button. */
	.message {
		font: inherit;
		font-size: var(--text-sm);
		background: none;
		border: none;
		padding: 0;
		cursor: pointer;
		max-width: 18rem;
		overflow: hidden;
		text-overflow: ellipsis;
		white-space: nowrap;
	}

	.message.ok {
		color: var(--color-fg-muted);
	}

	.message.error {
		color: var(--color-error-fg);
	}
</style>
