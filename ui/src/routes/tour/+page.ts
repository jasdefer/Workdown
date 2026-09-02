// Fetches everything the tour shows: the views index and project identity
// come from the layout; the data of every view the plan references is
// fetched here in parallel. A view that fails to load is left out of the
// map and its scene is skipped — a broken gantt view must not take the
// whole tour down with it. A 422 (the project itself cannot load) is
// surfaced the way every other page does.

import { error } from '@sveltejs/kit';
import { api } from '$lib/api/client';
import type { ViewData } from '$lib/api/generated/ViewData';
import { plannedViewIds, planTour } from '$lib/tour/plan';
import type { PageLoad } from './$types';

export const load: PageLoad = async ({ parent }) => {
	const layout = await parent();
	if (layout.viewsStatus === 422) {
		error(422, {
			message: 'The workdown project could not be loaded.',
			diagnostics: layout.layoutDiagnostics
		});
	}

	const plan = planTour(layout.views);
	const ids = plannedViewIds(plan);
	const results = await Promise.all(ids.map((id) => api.getView(id)));
	const data = new Map<string, ViewData>();
	results.forEach((result, index) => {
		const id = ids[index];
		if (result.data !== undefined && id !== undefined) data.set(id, result.data);
	});

	return { plan, data, views: layout.views, project: layout.project };
};
