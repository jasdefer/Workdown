<script lang="ts" module>
	function title(status: number): string {
		if (status === 404) return 'Not Found';
		if (status === 422) return 'Project Unloadable';
		return 'Error';
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
-->
<script lang="ts">
	import { page } from '$app/state';
	import type { Diagnostic } from '$lib/api/generated/Diagnostic';

	const status = $derived(page.status);
	const errorObj = $derived(page.error);
	const message = $derived(errorObj?.message ?? 'Something went wrong.');
	const errorDiagnostics = $derived<Diagnostic[]>(errorObj?.diagnostics ?? []);
	const views = $derived(page.data.views ?? []);
</script>

<section class="error-page">
	<header>
		<h1>{status} — {title(status)}</h1>
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
		<p class="refresh-hint">
			Try <a href={page.url.pathname}>refreshing the page</a>, or check that
			<code>workdown serve</code> is still running.
		</p>
	{/if}
</section>

<style>
	.error-page {
		max-width: 48rem;
		display: flex;
		flex-direction: column;
		gap: var(--space-6);
	}

	header h1 {
		font-size: var(--text-lg);
		font-weight: 600;
		margin: 0 0 var(--space-2);
	}

	.message {
		color: var(--color-fg-muted);
		margin: 0;
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

	.hint {
		color: var(--color-fg-muted);
		margin: 0;
	}

	.diagnostics li {
		display: flex;
		gap: var(--space-2);
		color: var(--color-warning-fg);
	}

	.diagnostics li.error {
		color: var(--color-error-fg);
	}

	.refresh-hint {
		color: var(--color-fg-muted);
		font-size: var(--text-sm);
	}
</style>
