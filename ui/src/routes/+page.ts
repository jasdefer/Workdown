// Root page: redirect to the first configured view, or fall through
// to the empty-state component when no views exist.

import { error, redirect } from '@sveltejs/kit';
import type { PageLoad } from './$types';

export const load: PageLoad = async ({ parent }) => {
	const layout = await parent();

	// The project failed to load (bad schema, unparseable item, …):
	// `/api/views` returned 422 with diagnostics and no views. Mirror
	// `/views/[id]` and surface the real error via `+error.svelte`,
	// rather than falling through to the misleading "no views
	// configured" empty state below.
	if (layout.viewsStatus === 422) {
		error(422, {
			message: 'The workdown project could not be loaded.',
			diagnostics: layout.layoutDiagnostics
		});
	}

	const first = layout.views[0];
	if (first) {
		redirect(307, `/views/${encodeURIComponent(first.id)}`);
	}
	// No views configured — `+page.svelte` renders the empty state.
	return {};
};
