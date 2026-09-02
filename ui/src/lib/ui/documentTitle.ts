// What the browser tab says, minus the timer's decoration: the project's
// name, then the page inside it. Pure so it can be tested without a DOM
// — the layout is the only caller, and it owns none of the reasoning.
//
// Project first, not page first: the complaint this answers is two
// workdown servers on two ports being indistinguishable in the tab
// strip, and a narrow tab shows only its first few characters. The
// window switcher and any bookmark inherit the same order.

import type { ViewSummary } from '$lib/api/generated/ViewSummary';
import { prettifyId, viewLabel } from '$lib/views/prettify';

/** The tab title before the project's name is known — the first paint,
 * and any tab whose `GET /api/project` never answered. The tool's own
 * name is the honest stand-in: it is what the app is. */
export const FALLBACK_TITLE = 'Workdown';

/** Separator between the project and the page within it. */
const SEPARATOR = ' — ';

/**
 * Name the page inside the project, or `null` for a route with nothing
 * worth naming (the root, which only redirects, and the error page).
 *
 * Every label here comes from data the route already has when the tab
 * is painted: the view list the layout fetched for the switcher, the id
 * in the URL, or a fixed word. Nothing waits on a second fetch — an
 * item page is titled from its prettified id, exactly as the page's own
 * heading is.
 *
 * The cases mirror `src/routes/` by hand — SvelteKit derives the route
 * ids from that directory layout, and nothing ties the two together.
 * When a route is added or renamed, add or rename its case here too;
 * a route this switch doesn't know silently gets the project-only title.
 */
export function pageLabel(
	routeId: string | null,
	params: Partial<Record<string, string>>,
	views: ViewSummary[]
): string | null {
	switch (routeId) {
		case '/views/[id]': {
			const id = params.id;
			if (id === undefined) return null;
			const view = views.find((candidate) => candidate.id === id);
			return view !== undefined ? viewLabel(view) : prettifyId(id);
		}
		case '/views/[id]/edit':
			return 'Edit view';
		case '/views/new':
			return 'New view';
		case '/items/new':
			return 'New item';
		case '/items/[id]': {
			const id = params.id;
			return id === undefined ? null : prettifyId(id);
		}
		default:
			return null;
	}
}

/**
 * The tab title: `Project — Page`, the project alone when the page has
 * no name of its own, and [`FALLBACK_TITLE`] whenever the project's
 * name is missing or blank.
 */
export function documentTitle(projectName: string | null | undefined, page: string | null): string {
	const project = projectName?.trim();
	const base = project !== undefined && project.length > 0 ? project : FALLBACK_TITLE;
	return page === null ? base : `${base}${SEPARATOR}${page}`;
}
