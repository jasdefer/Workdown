<!--
  Flat list of diagnostics with a severity icon per line. The mutation-
  surface counterpart to DiagnosticBanner: where the banner groups a
  whole project's findings by item for a view, this renders the diagnostics
  a single mutation response carried (save-with-warning warnings). Renders
  nothing when the list is empty.
-->
<script lang="ts">
	import type { Diagnostic } from '$lib/api/generated/Diagnostic';

	interface Props {
		diagnostics: Diagnostic[];
		label?: string;
	}

	let { diagnostics, label = 'Warnings' }: Props = $props();
</script>

{#if diagnostics.length > 0}
	<ul class="diagnostics" aria-label={label}>
		{#each diagnostics as diagnostic, index (index)}
			<li class:error={diagnostic.severity === 'error'}>
				<span class="icon" aria-hidden="true">{diagnostic.severity === 'error' ? '✕' : '⚠'}</span>
				<span class="message">{diagnostic.message}</span>
			</li>
		{/each}
	</ul>
{/if}

<style>
	.diagnostics {
		list-style: none;
		margin: 0;
		padding: var(--space-2);
		border: 1px solid var(--color-border);
		border-radius: var(--radius-md);
		background-color: var(--color-surface);
		font-size: var(--text-sm);
		display: flex;
		flex-direction: column;
		gap: 0.25rem;
	}

	li {
		color: var(--color-warning-fg);
		display: flex;
		gap: var(--space-2);
	}

	li.error {
		color: var(--color-error-fg);
	}

	.icon {
		font-weight: 700;
		flex-shrink: 0;
	}

	.message {
		min-width: 0;
		overflow-wrap: anywhere;
	}
</style>
