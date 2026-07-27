// Fetches the view data for /views/[id]. Maps 422/404 to SvelteKit
// `error()` so the route-level `+error.svelte` boundary renders.

import { error } from '@sveltejs/kit';
import { api } from '$lib/api/client';
import { displayOverrideParam, loadDisplayOverride } from '$lib/views/displayOverride';
import type { PageLoad } from './$types';

export const load: PageLoad = async ({ params, url }) => {
	// `?filter=` carries a JSON clause array for a "for right now" preview.
	// Passing it through to the view fetch re-narrows the result without
	// persisting; absent, the view renders with its saved filter.
	const filter = url.searchParams.get('filter');
	// The per-session display override lives in localStorage, not the URL —
	// re-read on every invalidation so it survives SSE-triggered reloads.
	const displayOverride = loadDisplayOverride(params.id);
	const result = await api.getView(
		params.id,
		filter ?? undefined,
		displayOverride !== null ? displayOverrideParam(displayOverride) : undefined
	);

	if (result.status === 422) {
		error(422, {
			message: 'The workdown project could not be loaded.',
			diagnostics: result.diagnostics
		});
	}
	if (result.status === 404) {
		error(404, {
			message: `View '${params.id}' is not configured in views.yaml.`,
			diagnostics: result.diagnostics
		});
	}

	return {
		viewId: params.id,
		itemId: url.searchParams.get('item'),
		filter,
		displayOverride,
		result
	};
};
