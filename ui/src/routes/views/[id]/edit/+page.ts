// Fetches the edit-form seed for /views/[id]/edit: the view's flat
// definition plus its filter as structured clauses — exactly the shape
// the save will PUT back. Maps 422/404 to SvelteKit `error()` so the
// route-level `+error.svelte` boundary renders.

import { error } from '@sveltejs/kit';
import { api } from '$lib/api/client';
import type { PageLoad } from './$types';

export const load: PageLoad = async ({ params }) => {
	const result = await api.getViewDefinition(params.id);

	if (result.status === 422) {
		error(422, {
			message: 'The views.yaml file could not be loaded.',
			diagnostics: result.diagnostics
		});
	}
	if (result.status === 404) {
		error(404, {
			message: `View '${params.id}' is not configured in views.yaml.`,
			diagnostics: result.diagnostics
		});
	}
	// Anything else without data (e.g. the 500 serialize-failure path)
	// surfaces with its own status and the server's error text — never
	// disguised as "not configured".
	if (result.data === undefined) {
		error(result.status, {
			message: result.error ?? 'Failed to load the view definition.',
			diagnostics: result.diagnostics
		});
	}

	return {
		viewId: params.id,
		seed: result.data
	};
};
