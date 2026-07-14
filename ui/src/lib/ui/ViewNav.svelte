<!--
  In-app view switcher in the app header: one link per configured view.
  The dominant workflow is flipping between a couple of views, so the
  list stays always-visible (no dropdown, no detour through a home page)
  and the current view is highlighted as a filled pill.

  Renders nothing when no views are configured — an empty switcher is
  noise, and the project may simply have none yet.

  Active detection uses `page.params.id` (the matched `/views/[id]`
  segment, already decoded) rather than comparing pathnames, so it stays
  correct regardless of id encoding and is empty on non-view pages.
-->
<script lang="ts">
	import { page } from '$app/state';
	import type { ViewSummary } from '$lib/api/generated/ViewSummary';
	import { viewLabel } from '$lib/views/prettify';
	import Chip from '$lib/ui/Chip.svelte';
	import ViewKindIcon from '$lib/ui/ViewKindIcon.svelte';

	interface Props {
		views: ViewSummary[];
	}

	let { views }: Props = $props();

	let activeId = $derived(page.params.id);
	let creating = $derived(page.url.pathname === '/views/new');
</script>

<nav class="view-nav" aria-label="Views">
	{#each views as view (view.id)}
		{@const isActive = view.id === activeId}
		<a
			class="view-link"
			href={`/views/${encodeURIComponent(view.id)}`}
			aria-current={isActive ? 'page' : undefined}
		>
			<Chip active={isActive} interactive>
				<span class="link-content">
					<ViewKindIcon kind={view.kind} />
					{viewLabel(view)}
				</span>
			</Chip>
		</a>
	{/each}

	<!-- The "Create view" entry point deferred by app-shell-navigation. -->
	<a class="view-link" href="/views/new" aria-current={creating ? 'page' : undefined}>
		<Chip active={creating} interactive>
			<span class="link-content">＋ New view</span>
		</Chip>
	</a>
</nav>

<style>
	/* `display: contents` dissolves the <nav> box so its view links
	   become direct items of the header's flex-wrap row. They then flow
	   inline after the brand and wrap onto additional rows individually,
	   rather than the whole nav dropping below the brand as one block.
	   The <nav> landmark + aria-label are preserved for assistive tech. */
	.view-nav {
		display: contents;
	}

	.view-link {
		text-decoration: none;
		border-radius: var(--radius-full);
	}

	.link-content {
		display: inline-flex;
		align-items: center;
		gap: var(--space-1);
	}
</style>
