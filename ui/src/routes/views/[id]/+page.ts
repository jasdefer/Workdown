// Fetches the view data for /views/[id]. Maps 422/404 to SvelteKit
// `error()` so the route-level `+error.svelte` boundary renders.

import { error } from '@sveltejs/kit';
import { api } from '$lib/api/client';
import type { PageLoad } from './$types';

export const load: PageLoad = async ({ params, url }) => {
	const result = await api.getView(params.id);

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
		result
	};
};
