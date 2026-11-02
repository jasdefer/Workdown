<script lang="ts" module>
	function title(status: number): string {
		if (status === 404) return 'Not found';
		if (status === 422) return 'Project unloadable';
		return 'Something went wrong';
	}
</script>

<!--
  Route-level error boundary. Catches `error()` thrown from `+page.ts`
  loads. Three cases handled (per the failure tiers in
  `first-view-end-to-end`):

  - 422: project loaded with a fatal problem (missing schema,
         unparseable views.yaml). Diagnostics ride along in
         `$page.error.diagnostics`.
  - 404: unknown view id. The message names the bad id; the header's
         view nav (rendered above this boundary) already lists the
         valid views, so we just point at it rather than repeat them.
  - everything else: network failure or unexpected throw.

  One centered column: the picture (ErrorArt, by status), the status
  as the big number, the title, the message, then whatever the case
  adds — a hint, the diagnostics, a refresh link.
-->
<script lang="ts">
	import { page } from '$app/state';
	import type { Diagnostic } from '$lib/api/generated/Diagnostic';
	import ErrorArt from '$lib/ui/ErrorArt.svelte';

	const status = $derived(page.status);
	const errorObj = $derived(page.error);
	const message = $derived(errorObj?.message ?? 'Something went wrong.');
	const errorDiagnostics = $derived<Diagnostic[]>(errorObj?.diagnostics ?? []);
	const views = $derived(page.data.views ?? []);
</script>

<section class="error-page">
	<div class="column">
		<ErrorArt httpStatus={status} />

		<header>
			<p class="status" aria-hidden="true">{status}</p>
			<h1>{title(status)}</h1>
			<p class="message">{message}</p>
		</header>

		{#if status === 404}
			<p class="hint">
				{#if views.length > 0}
					Pick a view from the navigation above.
				{:else}
					No views are configured yet — add one to <code>.workdown/views.yaml</code>.
				{/if}
			</p>
		{/if}

		{#if errorDiagnostics.length > 0}
			<section class="diagnostics">
				<h2>Diagnostics</h2>
				<ul>
					{#each errorDiagnostics as diagnostic, index (index)}
						<li class:error={diagnostic.severity === 'error'}>
							<span class="icon" aria-hidden="true">
								{diagnostic.severity === 'error' ? '✕' : '⚠'}
							</span>
							{diagnostic.message}
						</li>
					{/each}
				</ul>
			</section>
		{/if}

		{#if status !== 404 && status !== 422}
			<p class="hint">
				Try <a href={page.url.pathname}>refreshing the page</a>, or check that
				<code>workdown serve</code> is still running.
			</p>
		{/if}
	</div>
</section>

<style>
	/* Fills the content area the shell gives it and centers the column
	   in it, both ways; scrolls on its own if a diagnostics list is
	   taller than the window. */
	.error-page {
		flex: 1;
		min-height: 0;
		overflow-y: auto;
		display: grid;
		place-items: center;
		padding: var(--space-6) 0;
	}

	.column {
		max-width: 36rem;
		display: flex;
		flex-direction: column;
		align-items: center;
		text-align: center;
		gap: var(--space-4);
	}

	header {
		display: flex;
		flex-direction: column;
		align-items: center;
		gap: var(--space-1);
	}

	.status {
		font-family: var(--font-mono);
		font-size: 3rem;
		font-weight: 700;
		line-height: 1;
		letter-spacing: 0.04em;
		color: var(--color-accent);
		margin: 0;
	}

	h1 {
		font-size: var(--text-lg);
		font-weight: 600;
		margin: 0;
	}

	.message,
	.hint {
		color: var(--color-fg-muted);
		margin: 0;
	}

	.hint {
		font-size: var(--text-sm);
	}

	/* The list reads left to right like any list; only the column
	   around it is centered. */
	.diagnostics {
		width: 100%;
		text-align: left;
		background: var(--color-card);
		border: 1px solid var(--color-border);
		border-radius: var(--radius-md);
		padding: var(--space-4);
	}

	.diagnostics h2 {
		font-size: var(--text-base);
		font-weight: 600;
		margin: 0 0 var(--space-2);
	}

	.diagnostics ul {
		list-style: none;
		padding: 0;
		margin: 0;
		display: flex;
		flex-direction: column;
		gap: var(--space-2);
	}

	.diagnostics li {
		display: flex;
		gap: var(--space-2);
		color: var(--color-warning-fg);
	}

	.diagnostics li.error {
		color: var(--color-error-fg);
	}
</style>
