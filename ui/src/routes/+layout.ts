// Load the views index for every page. Used by:
//   - `+page.ts` at `/` to pick the landing view (first in the list).
//   - `+error.svelte` to render "did you mean…" alternatives on 404.
//   - the diagnostic banner / future nav menu.
//
// One round trip per navigation; SvelteKit caches `load()` per route
// so navigating between views doesn't re-fetch.

import { api } from '$lib/api/client';
import type { LayoutLoad } from './$types';

export const ssr = false;
export const prerender = false;

export const load: LayoutLoad = async () => {
	const result = await api.getViews();
	return {
		views: result.data ?? [],
		layoutDiagnostics: result.diagnostics
	};
};
