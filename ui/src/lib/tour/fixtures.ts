// Small view-data fixtures shared by the tour's tests.

import type { BoardData } from '$lib/api/generated/BoardData';
import type { Card } from '$lib/api/generated/Card';
import type { GanttData } from '$lib/api/generated/GanttData';
import type { GraphData } from '$lib/api/generated/GraphData';
import type { TreeData } from '$lib/api/generated/TreeData';
import type { TreeNode } from '$lib/api/generated/TreeNode';

export const card = (id: string, extra: Partial<Card> = {}): Card => ({
	id,
	title: null,
	subtitle: null,
	background: null,
	fields: [],
	body: '',
	...extra
});

export const node = (id: string, children: TreeNode[] = []): TreeNode => ({
	id,
	title: null,
	background: null,
	cells: [],
	children
});

export const board: BoardData = {
	field: 'status',
	field_type: 'choice',
	columns: [
		{ value: 'open', cards: [card('a'), card('b'), card('c')] },
		{ value: 'done', cards: [card('d')] },
		{ value: null, cards: [] }
	]
};

/** One root with two features; the first feature's children are all leaves. */
export const tree: TreeData = {
	field: 'parent',
	columns: [],
	roots: [
		node('epic', [node('feature-1', [node('t1'), node('t2'), node('t3')]), node('feature-2')])
	]
};

/** `b` and `c` both depend on `a`; `d` depends on `b`. */
export const graph: GraphData = {
	field: 'depends_on',
	group_by: null,
	nodes: [card('a'), card('b'), card('c'), card('d')],
	edges: [
		{ from: 'b', to: 'a' },
		{ from: 'c', to: 'a' },
		{ from: 'd', to: 'b' }
	],
	groups: null
};

export const gantt: GanttData = {
	group_field: null,
	bars: [
		{ card: card('early'), start: '2026-08-01', end: '2026-08-10', group: null },
		{ card: card('overlap'), start: '2026-08-05', end: '2026-08-20', group: null },
		{ card: card('late'), start: '2026-09-15', end: '2026-09-30', group: null }
	],
	unplaced: []
};
