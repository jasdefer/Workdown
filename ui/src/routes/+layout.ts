// Load what every page needs from the layout: the views index and the
// project's identity.
//
// The views index is used by:
//   - `+page.ts` at `/` to pick the landing view (first in the list).
//   - `+error.svelte` to render "did you mean…" alternatives on 404.
//   - the view switcher (`ViewNav`) and the diagnostic banner in
//     `+layout.svelte`.
//   - the browser tab title, for the current view's name.
//
// The project's identity titles the tab (and fills the description
// meta). Fetched in parallel with the views — it is a different endpoint
// on the same server, and serialising them would delay the first paint
// for nothing. It comes from the server's boot-time config rather than a
// project load, so a project that can't load still names its tab.
//
// One round trip each per navigation; SvelteKit caches `load()` per route
// so navigating between views doesn't re-fetch.

import { api } from '$lib/api/client';
import type { LayoutLoad } from './$types';

export const ssr = false;
export const prerender = false;

export const load: LayoutLoad = async () => {
	const [views, project] = await Promise.all([api.getViews(), api.getProject()]);
	return {
		views: views.data ?? [],
		viewsStatus: views.status,
		layoutDiagnostics: views.diagnostics,
		project: project.data ?? null
	};
};
