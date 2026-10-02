// Fetches the schema definition for /schema.
//
// Unlike the view pages this does not map a 422 to SvelteKit's `error()`:
// a schema that fails to load is the state this page exists to explain,
// so the whole result travels to the page and the page renders the load
// diagnostic in place of its sections. Every other read is in the 422
// tier at that moment; this is where the reason is legible.
//
// A page load function rather than the cached schema store the item
// editors use: the layout re-runs every load function on the file
// watcher's ping, so an edit to `schema.yaml` in a text editor refreshes
// the page for free, while the store is never re-fetched by the ping. No
// other page needs this payload.
//
// The field editor's state is the URL, as the item panel's is on a view
// page: `?field=<name>` opens the panel on that field, `?add` opens it
// empty. Both are read here so the page re-runs when they change.

import { api } from '$lib/api/client';
import type { PageLoad } from './$types';

export const load: PageLoad = async ({ url }) => {
	const result = await api.getSchemaDefinition();
	return {
		result,
		fieldName: url.searchParams.get('field'),
		adding: url.searchParams.has('add')
	};
};
