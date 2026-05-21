<!--
	Root landing page.

	The settled URL convention is "/" redirects to the first view in
	views.yaml. That redirect can't be wired today because no API
	endpoint exposes the views list (and we deliberately ship no
	endpoints in this slice). The redirect lands with
	first-view-end-to-end alongside the first view + a `GET /api/views`
	endpoint.

	Until then this is a deliberate stub: identifies what's missing
	rather than pretending the app has no front door.
-->

<section class="empty">
	<h1>Workdown</h1>
	<p>No views configured yet. The first view lands with the next slice.</p>
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
