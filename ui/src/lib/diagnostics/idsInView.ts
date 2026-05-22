// Set of work-item ids that appear in a rendered view payload. The
// DiagnosticBanner uses this to classify diagnostics as primary
// (referenced item is in the view) or secondary (item is elsewhere).
//
// Heatmap, bar chart, metric, and workload aggregate over items — they
// don't reference individual items at the wire level, so they return
// empty sets. Diagnostics about items aggregated into those views land
// in the secondary section.

import type { ViewData } from '$lib/api/generated/ViewData';
import type { WorkItemId } from '$lib/api/generated/WorkItemId';
import type { TreeNode } from '$lib/api/generated/TreeNode';
import type { TreemapNode } from '$lib/api/generated/TreemapNode';

export function idsInView(view: ViewData): Set<WorkItemId> {
	const ids = new Set<WorkItemId>();
	switch (view.type) {
		case 'board':
			for (const column of view.columns) {
				for (const card of column.cards) {
					ids.add(card.id);
				}
			}
			break;

		case 'table':
			for (const row of view.rows) {
				ids.add(row.id);
			}
			break;

		case 'tree':
			collectTreeNodeIds(view.roots, ids);
			break;

		case 'graph':
			for (const node of view.nodes) {
				ids.add(node.id);
			}
			if (view.groups) {
				collectTreeNodeIds(view.groups.roots, ids);
			}
			break;

		case 'gantt':
			for (const bar of view.bars) {
				ids.add(bar.card.id);
			}
			break;

		case 'gantt_by_depth':
			for (const level of view.levels) {
				for (const bar of level.bars) {
					ids.add(bar.card.id);
				}
			}
			break;

		case 'gantt_by_initiative':
			for (const initiative of view.initiatives) {
				ids.add(initiative.root.id);
				for (const bar of initiative.bars) {
					ids.add(bar.card.id);
				}
			}
			break;

		case 'treemap':
			collectTreemapNodeIds(view.root, ids);
			break;

		case 'line_chart':
			for (const point of view.points) {
				ids.add(point.id);
			}
			break;

		// Heatmap/bar-chart/metric/workload aggregate items into buckets;
		// individual item ids don't appear on the wire.
		case 'heatmap':
		case 'bar_chart':
		case 'metric':
		case 'workload':
			break;
	}
	return ids;
}

function collectTreeNodeIds(nodes: TreeNode[], ids: Set<WorkItemId>): void {
	for (const node of nodes) {
		ids.add(node.card.id);
		collectTreeNodeIds(node.children, ids);
	}
}

function collectTreemapNodeIds(node: TreemapNode, ids: Set<WorkItemId>): void {
	if (node.card) {
		ids.add(node.card.id);
	}
	for (const child of node.children) {
		collectTreemapNodeIds(child, ids);
	}
}
