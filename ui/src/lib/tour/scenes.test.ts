import { describe, expect, it } from 'vitest';
import type { ViewData } from '$lib/api/generated/ViewData';
import type { ViewSummary } from '$lib/api/generated/ViewSummary';
import { board, card, gantt, graph, node, tree } from './fixtures';
import { planTour } from './plan';
import { COMPACT_THRESHOLD, buildTour } from './scenes';
import type { TourInput } from './scenes';

const summary = (id: string, kind: ViewSummary['kind']): ViewSummary => ({ id, title: null, kind });

function input(views: ViewSummary[], data: Record<string, ViewData>): TourInput {
	return {
		views,
		plan: planTour(views),
		data: new Map(Object.entries(data)),
		project: { name: 'Northwind', description: null },
		viewport: { width: 1200, height: 800 },
		today: new Date('2026-09-02T00:00:00Z')
	};
}

const fullViews = [
	summary('status-board', 'board'),
	summary('hierarchy', 'tree'),
	summary('dependencies', 'graph'),
	summary('schedule', 'gantt'),
	summary('stats', 'metric')
];

const fullData: Record<string, ViewData> = {
	'status-board': { type: 'board', ...board },
	hierarchy: { type: 'tree', ...tree },
	dependencies: { type: 'graph', ...graph },
	schedule: { type: 'gantt', ...gantt },
	stats: {
		type: 'metric',
		rows: [
			{
				label: 'Items',
				aggregate: 'count',
				value_field: null,
				value: { type: 'number', value: 9 },
				unplaced: []
			}
		]
	}
};

describe('buildTour', () => {
	it('tells the whole story when every view kind is configured', () => {
		const { scenes } = buildTour(input(fullViews, fullData));
		expect(scenes.map((scene) => scene.name)).toEqual([
			'title',
			'swarm',
			'numbers',
			'structure',
			'by status',
			'dependencies',
			'timeline',
			'landing'
		]);
		const landing = scenes.at(-1);
		expect(landing?.landingViewId).toBe('status-board');
		expect(landing?.caption).toContain('Status Board');
	});

	it('collects every card once across all views', () => {
		const { cards } = buildTour(input(fullViews, fullData));
		const ids = cards.map((item) => item.id).sort();
		expect(new Set(ids).size).toBe(ids.length);
		expect(ids).toContain('epic');
		expect(ids).toContain('late');
		expect(ids).toContain('d');
	});

	it('gives every card a position in every scene', () => {
		const { cards, scenes } = buildTour(input(fullViews, fullData));
		for (const scene of scenes) {
			for (const item of cards) {
				expect(scene.layout.has(item.id)).toBe(true);
			}
		}
	});

	it('skips the timeline when there is no gantt view, and the structure when there is no tree', () => {
		const views = [summary('status-board', 'board')];
		const { scenes } = buildTour(input(views, { 'status-board': { type: 'board', ...board } }));
		expect(scenes.map((scene) => scene.name)).toEqual([
			'title',
			'swarm',
			'numbers',
			'by status',
			'landing'
		]);
	});

	it('skips the dependency scene when the graph has no edges', () => {
		const views = [summary('dependencies', 'graph')];
		const data: Record<string, ViewData> = {
			dependencies: { type: 'graph', ...graph, edges: [] }
		};
		const { scenes } = buildTour(input(views, data));
		expect(scenes.map((scene) => scene.name)).not.toContain('dependencies');
	});

	it('uses metric rows for the numbers, falling back to counts without a metric view', () => {
		const withMetric = buildTour(input(fullViews, fullData));
		const numbers = withMetric.scenes.find((scene) => scene.name === 'numbers');
		expect(numbers?.overlay).toEqual({ kind: 'metrics', tiles: [{ label: 'Items', value: '9' }] });

		const views = [summary('status-board', 'board')];
		const fallback = buildTour(input(views, { 'status-board': { type: 'board', ...board } }));
		const fallbackNumbers = fallback.scenes.find((scene) => scene.name === 'numbers');
		expect(fallbackNumbers?.overlay).toEqual({
			kind: 'metrics',
			tiles: [
				{ label: 'work items', value: '4' },
				{ label: 'open', value: '3' },
				{ label: 'done', value: '1' },
				{ label: 'no Status', value: '0' }
			]
		});
	});

	it('adds a second grouping scene only for a board on a different field', () => {
		const views = [
			summary('by-status', 'board'),
			summary('by-owner', 'board'),
			summary('again', 'board')
		];
		const owner: ViewData = {
			type: 'board',
			field: 'owner',
			field_type: 'string',
			columns: [{ value: 'ana', cards: [card('a')] }]
		};
		const { scenes } = buildTour(
			input(views, {
				'by-status': { type: 'board', ...board },
				'by-owner': owner,
				again: { type: 'board', ...board }
			})
		);
		expect(
			scenes.filter((scene) => scene.name.startsWith('by ')).map((scene) => scene.name)
		).toEqual(['by status', 'by owner']);
	});

	it('ends on the cloud when the landing view has no tour layout', () => {
		const views = [summary('everything', 'table'), summary('status-board', 'board')];
		const { scenes } = buildTour(input(views, { 'status-board': { type: 'board', ...board } }));
		const landing = scenes.at(-1);
		expect(landing?.landingViewId).toBe('everything');
		expect(landing?.labels).toEqual([]);
	});

	it('has no landing scene and no cards for a project without views', () => {
		const { cards, scenes } = buildTour(input([], {}));
		expect(cards).toEqual([]);
		expect(scenes.map((scene) => scene.name)).toEqual(['title', 'swarm', 'numbers']);
	});

	it('renders leaves as dots once the project is bigger than the threshold', () => {
		const leaves = Array.from({ length: COMPACT_THRESHOLD + 5 }, (_, index) =>
			node(`leaf-${index.toString()}`)
		);
		const bigTree: ViewData = {
			type: 'tree',
			field: 'parent',
			columns: [],
			roots: [node('root', leaves)]
		};
		const { cards } = buildTour(input([summary('hierarchy', 'tree')], { hierarchy: bigTree }));
		expect(cards.find((item) => item.id === 'root')?.compact).toBe(false);
		expect(cards.filter((item) => item.compact)).toHaveLength(COMPACT_THRESHOLD + 5);
	});
});
