// Which views feed which scene.
//
// The tour has no configuration of its own: `views.yaml` is it. Each
// scene is derived from the first view of a kind — the first `metric`
// view supplies the numbers, the first `tree` the structure, `board`
// views the grouping (up to two, distinct fields decided downstream),
// the first `graph` the dependencies, the first `gantt` the timeline.
// A kind that is not configured skips its scene; a project without date
// fields simply has no gantt view and therefore no timeline. The tour
// ends by opening the first view in the list, the same one `/` redirects
// to.

import type { ViewSummary } from '$lib/api/generated/ViewSummary';

export interface TourPlan {
	metric: string | null;
	tree: string | null;
	/** In views.yaml order; at most `MAX_BOARD_SCENES`. */
	boards: string[];
	graph: string | null;
	gantt: string | null;
	/** The view the tour opens at the end; null when none is configured. */
	landing: string | null;
}

export const MAX_BOARD_SCENES = 2;

export function planTour(views: ViewSummary[]): TourPlan {
	const first = (kind: ViewSummary['kind']): string | null =>
		views.find((view) => view.kind === kind)?.id ?? null;
	return {
		metric: first('metric'),
		tree: first('tree'),
		boards: views
			.filter((view) => view.kind === 'board')
			.slice(0, MAX_BOARD_SCENES)
			.map((view) => view.id),
		graph: first('graph'),
		gantt: first('gantt'),
		landing: views[0]?.id ?? null
	};
}

/** Every view id the plan needs data for, deduplicated, in fetch order. */
export function plannedViewIds(plan: TourPlan): string[] {
	const ids = [plan.metric, plan.tree, ...plan.boards, plan.graph, plan.gantt, plan.landing];
	return [...new Set(ids.filter((id): id is string => id !== null))];
}
