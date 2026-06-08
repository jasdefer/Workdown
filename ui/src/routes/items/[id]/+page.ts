// Stub load() — slice 2 swaps the body for `api.getItem(params.id)`.

import type { PageLoad } from './$types';

export const load: PageLoad = ({ params }) => {
	return { itemId: params.id };
};
