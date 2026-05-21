// Stub load() — returns the view id and the optional `?item=` selector.
// Slice 2 swaps the body for `api.getView(params.id)`.

import type { PageLoad } from './$types';

export const load: PageLoad = ({ params, url }) => {
	return {
		viewId: params.id,
		itemId: url.searchParams.get('item')
	};
};
