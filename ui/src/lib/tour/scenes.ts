// From fetched view data to the scene list.
//
// `buildTour` is the one place that knows the story: which scenes exist,
// in what order, what each says, and how the camera approaches it. It is
// pure — view data, viewport size and today's date in, cards and scenes
// out — so the story can be tested without a browser. Scenes whose
// source view is missing (or empty of the thing they show) are dropped,
// never rendered empty.

import type { Card } from '$lib/api/generated/Card';
import type { ProjectIdentity } from '$lib/api/generated/ProjectIdentity';
import type { TreeNode } from '$lib/api/generated/TreeNode';
import type { ViewData } from '$lib/api/generated/ViewData';
import type { ViewSummary } from '$lib/api/generated/ViewSummary';
import { formatChartValue, pluralize } from '$lib/views/format';
import { cardLabel, prettifyId, viewLabel } from '$lib/views/prettify';
import {
	boundsOf,
	cloudLayout,
	columnsLayout,
	graphLayout,
	parkMissing,
	timelineLayout,
	treeLayout
} from './layouts';
import { fitPose, offsetPose } from './motion';
import type { Viewport } from './motion';
import type { TourPlan } from './plan';
import type {
	CameraPose,
	Layout,
	MetricTile,
	Scene,
	TourCard,
	WorldEdge,
	WorldLabel
} from './types';

/** Above this many cards, leaf items render as dots — structure, not details. */
export const COMPACT_THRESHOLD = 150;

export interface TourInput {
	views: ViewSummary[];
	plan: TourPlan;
	/** Fetched `ViewData` by view id; a failed fetch is simply absent. */
	data: ReadonlyMap<string, ViewData>;
	project: ProjectIdentity | null;
	viewport: Viewport;
	today: Date;
}

export interface Tour {
	cards: TourCard[];
	scenes: Scene[];
}

const FRONT: CameraPose = { tx: 0, ty: 0, tz: 0, rx: 0, ry: 0 };

const baseScene = (name: string, layout: Layout): Scene => ({
	name,
	caption: null,
	holdMs: 6000,
	layout,
	edges: [],
	labels: [],
	camera: { enter: FRONT, hold: FRONT },
	flythrough: false,
	dim: 1,
	todayLine: false,
	overlay: null,
	landingViewId: null
});

/** Every card in the fetched data, deduplicated; a shape with a subtitle wins over one without. */
function collectCards(data: ReadonlyMap<string, ViewData>): Map<string, TourCard> {
	const cards = new Map<string, TourCard>();
	const add = (
		card: Pick<Card, 'id' | 'title' | 'background'> & { subtitle?: string | null }
	): void => {
		const existing = cards.get(card.id);
		const subtitle = card.subtitle ?? existing?.subtitle ?? null;
		cards.set(card.id, {
			id: card.id,
			title: existing?.title ?? cardLabel(card),
			subtitle,
			background: existing?.background ?? card.background,
			compact: false
		});
	};
	const walk = (node: TreeNode): void => {
		add(node);
		node.children.forEach(walk);
	};
	for (const view of data.values()) {
		switch (view.type) {
			case 'board':
				view.columns.forEach((column) => {
					column.cards.forEach(add);
				});
				break;
			case 'tree':
				view.roots.forEach(walk);
				break;
			case 'graph':
				view.nodes.forEach(add);
				break;
			case 'gantt':
				view.bars.forEach((bar) => {
					add(bar.card);
				});
				view.unplaced.forEach((entry) => {
					add(entry.card);
				});
				break;
			default:
				break;
		}
	}
	return cards;
}

function leafIds(roots: readonly TreeNode[]): Set<string> {
	const leaves = new Set<string>();
	const walk = (node: TreeNode): void => {
		if (node.children.length === 0) leaves.add(node.id);
		node.children.forEach(walk);
	};
	roots.forEach(walk);
	return leaves;
}

function metricTiles(input: TourInput, cardCount: number): MetricTile[] {
	const metric = input.plan.metric === null ? undefined : input.data.get(input.plan.metric);
	if (metric?.type === 'metric' && metric.rows.length > 0) {
		return metric.rows.map((row) => ({ label: row.label, value: formatChartValue(row.value) }));
	}
	// No metric view: the item count, and the board's column counts if there is a board.
	const tiles: MetricTile[] = [{ label: 'work items', value: cardCount.toString() }];
	const firstBoard = input.plan.boards[0];
	const board = firstBoard === undefined ? undefined : input.data.get(firstBoard);
	if (board?.type === 'board') {
		for (const column of board.columns) {
			tiles.push({
				label: column.value ?? `no ${prettifyId(board.field)}`,
				value: column.cards.length.toString()
			});
		}
	}
	return tiles;
}

function viewTitle(views: readonly ViewSummary[], id: string): string {
	const view = views.find((candidate) => candidate.id === id);
	return view === undefined ? prettifyId(id) : viewLabel(view);
}

export function buildTour(input: TourInput): Tour {
	const cardMap = collectCards(input.data);
	const tree = input.plan.tree === null ? undefined : input.data.get(input.plan.tree);
	if (cardMap.size > COMPACT_THRESHOLD && tree?.type === 'tree') {
		for (const id of leafIds(tree.roots)) {
			const card = cardMap.get(id);
			if (card) card.compact = true;
		}
	}
	const cards = [...cardMap.values()];
	const ids = cards.map((card) => card.id);
	const scenes: Scene[] = [];
	const frame = (layout: Layout, labels: readonly WorldLabel[] = []): CameraPose =>
		fitPose(boundsOf(layout, labels), input.viewport);

	// Title and fly-through share the cloud; the camera does the work.
	const cloud = cloudLayout(ids);
	scenes.push({
		...baseScene('title', cloud),
		holdMs: 3200,
		dim: 0.35,
		overlay: { kind: 'title' },
		camera: { enter: { ...FRONT, tz: -1800 }, hold: { ...FRONT, tz: -1500 } }
	});
	scenes.push({
		...baseScene('swarm', cloud),
		caption: `${pluralize(cards.length, 'work item')}. Every one is a Markdown file in the repo.`,
		holdMs: 7000,
		flythrough: true,
		camera: { enter: { ...FRONT, tz: -1200 }, hold: { ...FRONT, tz: 1900 } }
	});
	scenes.push({
		...baseScene('numbers', cloud),
		holdMs: 6000,
		dim: 0.18,
		overlay: { kind: 'metrics', tiles: metricTiles(input, cards.length) },
		camera: { enter: { ...FRONT, tz: -700 }, hold: { ...FRONT, tz: -900 } }
	});

	if (tree?.type === 'tree' && tree.roots.length > 0) {
		const { layout, edges } = treeLayout(tree);
		const hold = frame(layout);
		scenes.push({
			...baseScene('structure', parkMissing(layout, ids)),
			caption: `${pluralize(tree.roots.length, 'top-level item')}, organised by ${prettifyId(tree.field)}. This is how the work is structured.`,
			holdMs: 7000,
			edges,
			camera: { enter: offsetPose(hold, { rx: 28, ty: -60, tz: -250 }), hold }
		});
	}

	const seenBoardFields = new Set<string>();
	for (const boardId of input.plan.boards) {
		const board = input.data.get(boardId);
		if (board?.type !== 'board' || seenBoardFields.has(board.field)) continue;
		const { layout, labels } = columnsLayout(board);
		if (layout.size === 0) continue;
		const hold = frame(layout, labels);
		const isFirst = seenBoardFields.size === 0;
		seenBoardFields.add(board.field);
		scenes.push({
			...baseScene(`by ${board.field}`, parkMissing(layout, ids)),
			caption: isFirst
				? `Where things stand: the same cards grouped by ${prettifyId(board.field)}.`
				: `And grouped by ${prettifyId(board.field)}.`,
			holdMs: isFirst ? 6000 : 5000,
			labels,
			camera: {
				enter: offsetPose(hold, {
					ry: isFirst ? -22 : 0,
					tx: isFirst ? 0 : -200,
					ty: 120,
					tz: -150
				}),
				hold
			}
		});
	}

	const graph = input.plan.graph === null ? undefined : input.data.get(input.plan.graph);
	if (graph?.type === 'graph' && graph.edges.length > 0) {
		const { layout, edges, hub } = graphLayout(graph);
		const hold = frame(layout);
		const hubCard = hub === null ? undefined : cardMap.get(hub);
		scenes.push({
			...baseScene('dependencies', parkMissing(layout, ids)),
			caption:
				`What blocks what, via ${prettifyId(graph.field)}.` +
				(hubCard ? ` “${hubCard.title}” is what most others wait on.` : ''),
			holdMs: 7000,
			edges,
			camera: { enter: offsetPose(hold, { ry: 30, tz: -200 }), hold }
		});
	}

	const gantt = input.plan.gantt === null ? undefined : input.data.get(input.plan.gantt);
	if (gantt?.type === 'gantt') {
		const { layout, labels, hasBars } = timelineLayout(gantt, input.today);
		if (hasBars) {
			const hold = frame(layout, labels);
			scenes.push({
				...baseScene('timeline', parkMissing(layout, ids)),
				caption: `When: ${pluralize(layout.size, 'scheduled item')} along the calendar, today marked.`,
				holdMs: 7000,
				labels,
				todayLine: true,
				camera: { enter: offsetPose(hold, { tx: 400, ty: 40, tz: -150 }), hold }
			});
		}
	}

	if (input.plan.landing !== null) {
		const landing = landingScene(input, input.plan.landing, ids, cloud, frame);
		scenes.push(landing);
	}

	return { cards, scenes };
}

/**
 * The last scene morphs into the layout of the view the tour then opens,
 * so the hand-off to the real app is a continuation, not a cut. A view
 * kind the tour has no layout for (a table, a chart) ends on the cloud.
 */
function landingScene(
	input: TourInput,
	landingId: string,
	ids: readonly string[],
	cloud: Layout,
	frame: (layout: Layout, labels?: readonly WorldLabel[]) => CameraPose
): Scene {
	const data = input.data.get(landingId);
	const title = viewTitle(input.views, landingId);
	let layout: Layout = cloud;
	let labels: WorldLabel[] = [];
	let edges: WorldEdge[] = [];
	let hold: CameraPose = { ...FRONT, tz: -900 };
	if (data?.type === 'board') {
		const result = columnsLayout(data);
		layout = parkMissing(result.layout, ids);
		labels = result.labels;
		hold = frame(result.layout, labels);
	} else if (data?.type === 'tree' && data.roots.length > 0) {
		const result = treeLayout(data);
		layout = parkMissing(result.layout, ids);
		edges = result.edges;
		hold = frame(result.layout);
	} else if (data?.type === 'graph' && data.nodes.length > 0) {
		const result = graphLayout(data);
		layout = parkMissing(result.layout, ids);
		edges = result.edges;
		hold = frame(result.layout);
	}
	return {
		...baseScene('landing', layout),
		caption: `This is “${title}”, where the real app begins.`,
		holdMs: 5000,
		labels,
		edges,
		camera: { enter: offsetPose(hold, { tz: -100 }), hold },
		landingViewId: landingId
	};
}
