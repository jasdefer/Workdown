<!--
  Root page. Only renders when no views are configured — otherwise
  `+page.ts` redirects to the first view in `views.yaml`.
-->

<section class="empty">
	<h1>Workdown</h1>
	<p>
		No views configured yet. Add one to <code>.workdown/views.yaml</code> to get started.
	</p>
</section>

<style>
	.empty {
		max-width: 36rem;
		display: flex;
		flex-direction: column;
		gap: var(--space-3);
	}

	.empty h1 {
		font-size: var(--text-lg);
		font-weight: 600;
	}

	.empty p {
		color: var(--color-fg-muted);
	}
</style>
