// Layouts: view data in, world positions out.
//
// Every function here is pure and DOM-free — `ViewData` from the server
// (already filtered, grouped and display-resolved the way the real view
// shows it) becomes a `Layout` in world pixels. The tour never re-derives
// grouping or gantt bars itself; a scene shows exactly what its source
// view shows, just arranged for the camera.
//
// World space: origin at the stage centre, x right, y down, z toward the
// viewer. A card's position is its centre. Cards a layout has no place
// for are parked out of sight by `parkMissing`.

import dagre from 'dagre';
import type { BoardData } from '$lib/api/generated/BoardData';
import type { GanttData } from '$lib/api/generated/GanttData';
import type { GraphData } from '$lib/api/generated/GraphData';
import type { TreeData } from '$lib/api/generated/TreeData';
import type { TreeNode } from '$lib/api/generated/TreeNode';
import { noValueLabel } from '$lib/views/prettify';
import type { Bounds, Layout, Position, WorldEdge, WorldLabel } from './types';

export const CARD_WIDTH = 150;
export const CARD_HEIGHT = 60;

const DAY_MS = 86_400_000;

/** Where cards go when a scene has no place for them: behind, invisible. */
const PARKED: Position = { x: 0, y: 700, z: -900, opacity: 0 };

const visible = (x: number, y: number, z = 0): Position => ({ x, y, z, opacity: 1 });

/** Deterministic pseudo-random in [0, 1): the cloud must look the same on every visit. */
function seededRandom(seed: number): () => number {
	let state = seed >>> 0;
	return () => {
		state = (state * 1_664_525 + 1_013_904_223) >>> 0;
		return state / 4_294_967_296;
	};
}

/** Cards scattered through a deep volume for the title and fly-through scenes. */
export function cloudLayout(ids: readonly string[], seed = 99): Layout {
	const random = seededRandom(seed);
	const layout: Layout = new Map();
	for (const id of ids) {
		layout.set(id, visible((random() - 0.5) * 1600, (random() - 0.5) * 900, 300 - random() * 2500));
	}
	return layout;
}

export interface ColumnsResult {
	layout: Layout;
	labels: WorldLabel[];
}

/**
 * The board's columns, headers above. A column with many cards wraps into
 * a block of sub-columns rather than one tall stack: a hundred done items
 * as a single column would fit the camera as a sliver, while a block
 * roughly 3:2 keeps the cards legible and still reads as "this pile is
 * the big one". Column widths therefore vary; the whole board is centred.
 */
export function columnsLayout(board: BoardData): ColumnsResult {
	const columnGap = 60;
	const cardGapX = 10;
	const gapY = CARD_HEIGHT + 14;
	const headerY = -CARD_HEIGHT;
	const layout: Layout = new Map();
	const labels: WorldLabel[] = [];

	const blocks = board.columns.map((column) => {
		const count = column.cards.length;
		const rows = Math.max(1, Math.ceil(Math.sqrt(count * 1.5)));
		const subColumns = Math.max(1, Math.ceil(count / rows));
		return { column, rows, width: subColumns * (CARD_WIDTH + cardGapX) - cardGapX };
	});
	const totalWidth =
		blocks.reduce((sum, block) => sum + block.width, 0) + columnGap * (blocks.length - 1);

	let left = -totalWidth / 2;
	for (const { column, rows, width } of blocks) {
		column.cards.forEach((card, index) => {
			const subColumn = Math.floor(index / rows);
			const row = index % rows;
			layout.set(
				card.id,
				visible(left + subColumn * (CARD_WIDTH + cardGapX) + CARD_WIDTH / 2, row * gapY)
			);
		});
		labels.push({
			x: left,
			y: headerY,
			text: column.value ?? noValueLabel(board.field),
			align: 'start',
			tone: 'muted'
		});
		labels.push({
			x: left + width,
			y: headerY,
			text: column.cards.length.toString(),
			align: 'end',
			tone: 'muted'
		});
		left += width + columnGap;
	}
	return { layout, labels };
}

export interface TreeResult {
	layout: Layout;
	edges: WorldEdge[];
}

/**
 * The hierarchy, roots on top. Siblings that are all leaves stack
 * vertically under their parent instead of spreading sideways — a tidy
 * tree with one column per leaf is many thousands of pixels wide for any
 * real project and unreadable from a camera that has to fit it all.
 */
export function treeLayout(tree: TreeData): TreeResult {
	const gapX = CARD_WIDTH + 24;
	const levelY = 120;
	const stackY = CARD_HEIGHT + 8;
	const layout: Layout = new Map();
	const edges: WorldEdge[] = [];

	const isLeaf = (node: TreeNode): boolean => node.children.length === 0;
	const stacksChildren = (node: TreeNode): boolean =>
		node.children.length > 0 && node.children.every(isLeaf);
	/** Horizontal span in columns. */
	const span = (node: TreeNode): number => {
		if (isLeaf(node) || stacksChildren(node)) return 1;
		return node.children.reduce((sum, child) => sum + span(child), 0);
	};

	const place = (node: TreeNode, left: number, y: number): void => {
		const centre = left + (span(node) * gapX) / 2;
		const position = visible(centre, y);
		layout.set(node.id, position);
		if (stacksChildren(node)) {
			node.children.forEach((child, row) => {
				const childPosition = visible(centre, y + levelY + row * stackY);
				layout.set(child.id, childPosition);
				edges.push({ from: position, to: childPosition, direction: 'down', hot: false });
			});
			return;
		}
		let cursor = left;
		for (const child of node.children) {
			place(child, cursor, y + levelY);
			cursor += span(child) * gapX;
			const childPosition = layout.get(child.id);
			if (childPosition) {
				edges.push({ from: position, to: childPosition, direction: 'down', hot: false });
			}
		}
	};

	const totalWidth = tree.roots.reduce((sum, root) => sum + span(root), 0) * gapX;
	let cursor = -totalWidth / 2;
	for (const root of tree.roots) {
		place(root, cursor, 0);
		cursor += span(root) * gapX;
	}
	return { layout, edges };
}

export interface GraphResult {
	layout: Layout;
	edges: WorldEdge[];
	/** The most depended-upon item, if any edge exists. */
	hub: string | null;
}

/**
 * The dependency graph, prerequisites on the left. A server edge runs
 * from an item to what it depends on; dagre gets it reversed so the flow
 * reads left→right. Rendering-only positions, so dagre's own node
 * ordering is all the crossing minimisation the tour needs.
 */
export function graphLayout(graph: GraphData): GraphResult {
	const dagreGraph = new dagre.graphlib.Graph();
	dagreGraph.setGraph({ rankdir: 'LR', nodesep: 30, ranksep: 90, marginx: 0, marginy: 0 });
	dagreGraph.setDefaultEdgeLabel(() => ({}));
	for (const node of graph.nodes) {
		dagreGraph.setNode(node.id, { width: CARD_WIDTH, height: CARD_HEIGHT });
	}
	const inbound = new Map<string, number>();
	for (const edge of graph.edges) {
		dagreGraph.setEdge(edge.to, edge.from);
		inbound.set(edge.to, (inbound.get(edge.to) ?? 0) + 1);
	}
	dagre.layout(dagreGraph);

	const layoutGraph = dagreGraph.graph();
	const width = layoutGraph.width ?? 0;
	const height = layoutGraph.height ?? 0;
	const layout: Layout = new Map();
	for (const node of graph.nodes) {
		const placed = dagreGraph.node(node.id);
		layout.set(node.id, visible(placed.x - width / 2, placed.y - height / 2));
	}

	let hub: string | null = null;
	let hubCount = 0;
	for (const [id, count] of inbound) {
		if (count > hubCount) {
			hub = id;
			hubCount = count;
		}
	}
	const edges: WorldEdge[] = [];
	for (const edge of graph.edges) {
		const from = layout.get(edge.to);
		const to = layout.get(edge.from);
		if (from && to) edges.push({ from, to, direction: 'right', hot: edge.to === hub });
	}
	return { layout, edges, hub };
}

export interface TimelineResult {
	layout: Layout;
	labels: WorldLabel[];
	/** Months and today only make sense when at least one bar was placed. */
	hasBars: boolean;
}

/**
 * Gantt bars as cards along a calendar axis, packed into rows. `today`
 * sits at x = 0 so the today line is the world's y axis.
 */
export function timelineLayout(gantt: GanttData, today: Date): TimelineResult {
	const layout: Layout = new Map();
	const labels: WorldLabel[] = [];
	const bars = gantt.bars
		.map((bar) => ({ id: bar.card.id, start: Date.parse(bar.start), end: Date.parse(bar.end) }))
		.filter((bar) => Number.isFinite(bar.start) && Number.isFinite(bar.end))
		.sort((left, right) => left.start - right.start);
	if (bars.length === 0) return { layout, labels, hasBars: false };

	const todayMs = today.getTime();
	const first = Math.min(todayMs, ...bars.map((bar) => bar.start));
	const last = Math.max(todayMs, ...bars.map((bar) => bar.end));
	const spanDays = Math.max(1, (last - first) / DAY_MS);
	// Wide enough to separate bars, narrow enough that a year still fits a screen.
	const pixelsPerDay = Math.min(12, Math.max(2, 1400 / spanDays));
	const toX = (ms: number): number => ((ms - todayMs) / DAY_MS) * pixelsPerDay;

	// Greedy row packing: a bar takes the first row whose last bar ended
	// before it starts, so overlapping work stacks and gaps are reused.
	const rowEnds: number[] = [];
	for (const bar of bars) {
		let row = rowEnds.findIndex((end) => end <= bar.start);
		if (row === -1) {
			row = rowEnds.length;
			rowEnds.push(0);
		}
		// Cards are fixed width; reserve at least a card's worth of days.
		rowEnds[row] = Math.max(bar.end, bar.start + (CARD_WIDTH / pixelsPerDay) * DAY_MS);
		layout.set(bar.id, visible(toX((bar.start + bar.end) / 2), row * (CARD_HEIGHT + 10)));
	}

	const axisY = -CARD_HEIGHT - 10;
	labels.push({ x: 0, y: axisY - 26, text: 'today', align: 'center', tone: 'accent' });
	const month = new Date(first);
	month.setUTCDate(1);
	month.setUTCHours(0, 0, 0, 0);
	for (; month.getTime() <= last; month.setUTCMonth(month.getUTCMonth() + 1)) {
		labels.push({
			x: toX(month.getTime()),
			y: axisY,
			text: month.toLocaleString('en', { month: 'short', year: 'numeric', timeZone: 'UTC' }),
			align: 'center',
			tone: 'muted'
		});
	}
	return { layout, labels, hasBars: true };
}

/** Fill in every id the layout lacks so no card is left where the previous scene put it. */
export function parkMissing(layout: Layout, ids: readonly string[]): Layout {
	const complete: Layout = new Map(layout);
	for (const id of ids) {
		if (!complete.has(id)) complete.set(id, PARKED);
	}
	return complete;
}

/** Extent of the visible cards, including the card body around each centre. */
export function boundsOf(layout: Layout, labels: readonly WorldLabel[] = []): Bounds {
	const bounds: Bounds = { minX: Infinity, maxX: -Infinity, minY: Infinity, maxY: -Infinity };
	for (const position of layout.values()) {
		if (position.opacity === 0) continue;
		bounds.minX = Math.min(bounds.minX, position.x - CARD_WIDTH / 2);
		bounds.maxX = Math.max(bounds.maxX, position.x + CARD_WIDTH / 2);
		bounds.minY = Math.min(bounds.minY, position.y - CARD_HEIGHT / 2);
		bounds.maxY = Math.max(bounds.maxY, position.y + CARD_HEIGHT / 2);
	}
	for (const label of labels) {
		bounds.minX = Math.min(bounds.minX, label.x);
		bounds.maxX = Math.max(bounds.maxX, label.x);
		bounds.minY = Math.min(bounds.minY, label.y - 16);
	}
	if (!Number.isFinite(bounds.minX)) return { minX: 0, maxX: 0, minY: 0, maxY: 0 };
	return bounds;
}

/** SVG path for an edge: a cubic from the near edge of one card to the near edge of the other. */
export function edgePath(edge: WorldEdge): string {
	const { from, to } = edge;
	const point = (x: number, y: number): string => `${x.toFixed(1)},${y.toFixed(1)}`;
	if (edge.direction === 'down') {
		const startY = from.y + CARD_HEIGHT / 2;
		const endY = to.y - CARD_HEIGHT / 2;
		const midY = (startY + endY) / 2;
		return `M${point(from.x, startY)} C${point(from.x, midY)} ${point(to.x, midY)} ${point(to.x, endY)}`;
	}
	const startX = from.x + CARD_WIDTH / 2;
	const endX = to.x - CARD_WIDTH / 2;
	const midX = (startX + endX) / 2;
	return `M${point(startX, from.y)} C${point(midX, from.y)} ${point(midX, to.y)} ${point(endX, to.y)}`;
}
