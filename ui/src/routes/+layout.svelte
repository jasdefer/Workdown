<script lang="ts">
	import '../app.css';
	import { onMount } from 'svelte';
	import { invalidateAll } from '$app/navigation';
	import { page } from '$app/state';
	import type { Snippet } from 'svelte';
	import type { LayoutData } from './$types';
	import favicon from '$lib/assets/favicon.svg';
	import GitSync from '$lib/git/GitSync.svelte';
	import { gitStore } from '$lib/stores/git.svelte';
	import { timerStore } from '$lib/stores/timer.svelte';
	import TimerPill from '$lib/timer/TimerPill.svelte';
	import TimerToast from '$lib/timer/TimerToast.svelte';
	import { documentTitle, pageLabel } from '$lib/ui/documentTitle';
	import ThemeToggle from '$lib/ui/ThemeToggle.svelte';
	import ViewNav from '$lib/ui/ViewNav.svelte';

	interface Props {
		data: LayoutData;
		children: Snippet;
	}

	let { data, children }: Props = $props();

	// The tab title without the timer's decoration: the project's name,
	// then whatever this route is. Both halves come from data already at
	// hand — the fetched project identity and the views index the switcher
	// uses — so the title never waits on a fetch of its own.
	const baseTitle = $derived(
		documentTitle(data.project?.name, pageLabel(page.route.id, page.params, data.views))
	);
	const projectDescription = $derived(data.project?.description ?? null);

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
			// Every file change moves the git dirty count; recount it
			// (locally — no remote traffic) alongside the page refetch.
			void gitStore.refresh();
		};
		// Timer changes arrive as a *named* event so the generic handler
		// above never fires for them: a timer action refetches the timer
		// state alone, and a file save never refetches the timer.
		source.addEventListener('timer', () => {
			void timerStore.reload();
		});
		// Repository movement (a commit, fetch, or branch switch — often
		// made in a terminal, which touches only `.git` and therefore
		// never fires the file-change ping) also travels named: it
		// refreshes the git pill alone, not the page.
		source.addEventListener('git', () => {
			void gitStore.refresh();
		});
		void timerStore.load();
		void gitStore.load();
		return () => {
			source.close();
		};
	});
</script>

<svelte:head>
	<!-- `Project — Page`, project first: two workdown servers on two
	     ports are told apart by the half of the title a narrow tab still
	     shows. The timer store decorates it with the pomodoro countdown
	     and an alarm form at zero — the "visible in the tab itself"
	     channel of the timer notifications. Interpolated, so a project
	     name is text and never markup. -->
	<title>{timerStore.documentTitle(baseTitle)}</title>
	{#if projectDescription !== null}
		<meta name="description" content={projectDescription} />
	{/if}
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
			<a
				class="header-link tour-link"
				href="/tour"
				title="Animated project overview"
				aria-current={page.route.id === '/tour' ? 'page' : undefined}>▶ Tour</a
			>
			<GitSync />
			<TimerPill />
			<a class="header-link" href="/items/new">+ New item</a>
			<ThemeToggle />
		</div>
	</header>
	<main class="app-main">
		{@render children()}
	</main>
	<TimerToast />
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

	/* The quiet text links in the actions row: the tour and the new-item
	   form. Same weight as the theme toggle, so they read as utilities. */
	.header-link {
		font-size: var(--text-sm);
		color: var(--color-fg-muted);
		text-decoration: none;
	}

	.header-link:hover {
		color: var(--color-fg);
	}

	.tour-link[aria-current='page'] {
		color: var(--color-fg);
	}

	/* Flex container so view-page's `flex: 1` can constrain against
	   a known height — that's what lets columns scroll independently
	   instead of the whole page scrolling. Positioned so overlays that
	   belong below the header (the item slide-over) can anchor to it
	   instead of the viewport — the header, and the timer pill's
	   expanded panel, stay visible above them. */
	.app-main {
		position: relative;
		flex: 1;
		min-height: 0;
		padding: var(--space-6);
		display: flex;
		flex-direction: column;
		overflow: hidden;
	}
</style>
