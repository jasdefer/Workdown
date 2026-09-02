<!--
  The git sync pill — the header's pull/push surface, shown only when
  the project opted in (`serve.git_controls: true`) and sits inside a
  git repository. Shows the branch and a glanceable summary
  (`↓behind ↑ahead · N local`, or `in sync`), a Pull button that is
  enabled only while the tree is clean — pull never touches uncommitted
  work — and a Push button that is enabled only when local commits
  exist; the tooltips and the dirty hint say why either is off. On a
  branch with no upstream the summary reads `not published` and the
  same button reads `Publish`: the first push also creates the remote
  branch, which the server handles. When the remote couldn't be
  reached, a hint appears whose click retries.

  Staleness is the server's problem, not this component's: it watches
  the repository's git directory and pings the git-named live-update
  event on any movement (wired to a refresh in the root layout), so a
  terminal-side commit shows up here without this window having to
  regain focus.

  Display rules live in `gitPill.ts` (unit-tested); operations and
  state live in the git store.
-->
<script lang="ts">
	import { gitStore } from '$lib/stores/git.svelte';
	import { pillModel } from './gitPill';

	const model = $derived(pillModel(gitStore.status, gitStore.busy));
</script>

{#if model.visible}
	<div class="header-pill" title={model.dirtyHint ?? undefined}>
		<span class="branch">{model.branch}</span>
		<span class="summary">{model.summary}</span>
		{#if model.remoteHint !== null}
			<button
				class="remote-hint"
				title={`${model.remoteHint} — click to try again`}
				onclick={() => void gitStore.retryRemote()}
			>
				remote unreachable ↻
			</button>
		{/if}
		<button
			class="action"
			onclick={() => void gitStore.pull()}
			disabled={!model.canPull}
			title={model.pullTitle}
		>
			Pull
		</button>
		<button
			class="action"
			onclick={() => void gitStore.push()}
			disabled={!model.canPush}
			title={model.pushTitle}
		>
			{model.pushLabel}
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

	.remote-hint {
		font: inherit;
		font-size: var(--text-sm);
		color: var(--color-warning-fg);
		background: none;
		border: none;
		padding: 0;
		cursor: pointer;
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
