<script lang="ts">
	import '../app.css';
	import { onMount } from 'svelte';
	import { invalidateAll } from '$app/navigation';
	import type { Snippet } from 'svelte';
	import type { LayoutData } from './$types';
	import favicon from '$lib/assets/favicon.svg';
	import ThemeToggle from '$lib/ui/ThemeToggle.svelte';
	import ViewNav from '$lib/ui/ViewNav.svelte';

	interface Props {
		data: LayoutData;
		children: Snippet;
	}

	let { data, children }: Props = $props();

	// One live-update pipe per tab. The server pushes a contentless ping
	// on any work-item or config file change (editor save, CLI mutation,
	// `git pull`, another tab's edit). We respond by re-running every load
	// function for the current page, which re-fetches and re-renders the
	// view in place — no full-page reload. `EventSource` reconnects on its
	// own if the stream drops; the cleanup closes it when the tab unmounts.
	// `onMount` runs only in the browser, so the `EventSource` global is safe.
	onMount(() => {
		const source = new EventSource('/api/events');
		source.onmessage = () => {
			void invalidateAll();
		};
		return () => {
			source.close();
		};
	});
</script>

<svelte:head>
	<title>Workdown</title>
	<link rel="icon" href={favicon} />
</svelte:head>

<div class="shell">
	<header class="app-header">
		<div class="header-left">
			<a class="brand" href="/">Workdown</a>
			<ViewNav views={data.views} />
			<!-- Reserved slot for future non-view destinations (dynamic view
			     generator, diagnostics, schema). Lives outside <ViewNav> so it
			     still shows when no views are configured; populated by later
			     issues. -->
		</div>
		<div class="header-actions">
			<a class="new-item" href="/items/new">+ New item</a>
			<ThemeToggle />
		</div>
	</header>
	<main class="app-main">
		{@render children()}
	</main>
</div>

<style>
	.shell {
		display: flex;
		flex-direction: column;
		height: 100vh;
	}

	.app-header {
		display: flex;
		align-items: center;
		justify-content: space-between;
		gap: var(--space-4);
		padding: var(--space-3) var(--space-6);
		background-color: var(--color-surface);
		border-bottom: 1px solid var(--color-border);
		flex-shrink: 0;
	}

	/* Wrapping row holding the brand and (via the nav's `display:
	   contents`) the individual view links. The first link sits beside
	   the brand; overflow wraps onto further rows starting at the brand's
	   left edge. Takes the space left of the pinned-right actions. */
	.header-left {
		display: flex;
		align-items: center;
		flex-wrap: wrap;
		gap: var(--space-2) var(--space-3);
		flex: 1 1 auto;
		min-width: 0;
	}

	.brand {
		font-weight: 600;
		color: var(--color-fg);
		text-decoration: none;
		flex-shrink: 0;
	}

	.header-actions {
		display: flex;
		align-items: center;
		gap: var(--space-3);
		flex-shrink: 0;
	}

	.new-item {
		font-size: var(--text-sm);
		color: var(--color-fg-muted);
		text-decoration: none;
	}

	.new-item:hover {
		color: var(--color-fg);
	}

	/* Flex container so view-page's `flex: 1` can constrain against
	   a known height — that's what lets columns scroll independently
	   instead of the whole page scrolling. */
	.app-main {
		flex: 1;
		min-height: 0;
		padding: var(--space-6);
		display: flex;
		flex-direction: column;
		overflow: hidden;
	}
</style>
