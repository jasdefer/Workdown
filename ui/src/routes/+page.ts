// Root page: redirect to the first configured view, or fall through
// to the empty-state component when no views exist.

import { redirect } from '@sveltejs/kit';
import type { PageLoad } from './$types';

export const load: PageLoad = ({ parent }) => {
	return parent().then((layout) => {
		const first = layout.views[0];
		if (first) {
			redirect(307, `/views/${encodeURIComponent(first.id)}`);
		}
		// No views configured — `+page.svelte` renders the empty state.
		return {};
	});
};
