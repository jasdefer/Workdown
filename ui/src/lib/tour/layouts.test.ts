import { describe, expect, it } from 'vitest';
import { board, card, gantt, graph, tree } from './fixtures';
import {
	CARD_HEIGHT,
	CARD_WIDTH,
	boundsOf,
	cloudLayout,
	columnsLayout,
	edgePath,
	graphLayout,
	parkMissing,
	timelineLayout,
	treeLayout
} from './layouts';
import type { Position } from './types';

const at = (layout: Map<string, Position>, id: string): Position => {
	const position = layout.get(id);
	if (position === undefined) throw new Error(`no position for ${id}`);
	return position;
};

describe('cloudLayout', () => {
	it('is deterministic and spreads cards through depth', () => {
		const ids = ['a', 'b', 'c', 'd'];
		const first = cloudLayout(ids);
		const second = cloudLayout(ids);
		expect([...first.entries()]).toEqual([...second.entries()]);
		const depths = ids.map((id) => at(first, id).z);
		expect(Math.max(...depths) - Math.min(...depths)).toBeGreaterThan(200);
	});
});

describe('columnsLayout', () => {
	it('stacks a small column downward, orders columns left to right, and centres the board', () => {
		const { layout, labels } = columnsLayout(board);
		expect(at(layout, 'a').y).toBeLessThan(at(layout, 'b').y);
		expect(at(layout, 'b').y).toBeLessThan(at(layout, 'c').y);
		expect(at(layout, 'a').x).toBe(at(layout, 'c').x);
		expect(at(layout, 'a').x).toBeLessThan(at(layout, 'd').x);
		// Three equal-width columns: the middle one sits on the axis.
		expect(at(layout, 'd').x).toBeCloseTo(0);
		expect(labels.map((label) => label.text)).toEqual([
			'open',
			'3',
			'done',
			'1',
			'(no Status)',
			'0'
		]);
	});

	it('wraps a tall column into a block of sub-columns', () => {
		const cards = Array.from({ length: 24 }, (_, index) => card(`c${index.toString()}`));
		const { layout } = columnsLayout({ ...board, columns: [{ value: 'done', cards }] });
		// 24 cards → 6 rows (ceil √36) × 4 sub-columns.
		expect(at(layout, 'c5').y).toBeGreaterThan(at(layout, 'c4').y);
		expect(at(layout, 'c6').y).toBe(at(layout, 'c0').y);
		expect(at(layout, 'c6').x).toBeGreaterThan(at(layout, 'c0').x);
		const bounds = boundsOf(layout);
		expect(Math.abs(bounds.minX + bounds.maxX)).toBeLessThan(1);
		expect(bounds.maxY - bounds.minY).toBeLessThan(bounds.maxX - bounds.minX);
	});
});

describe('treeLayout', () => {
	it('places children below their parent and stacks all-leaf siblings vertically', () => {
		const { layout, edges } = treeLayout(tree);
		expect(at(layout, 'epic').y).toBeLessThan(at(layout, 'feature-1').y);
		expect(at(layout, 'feature-1').y).toBe(at(layout, 'feature-2').y);
		// Leaves under feature-1 share its x and descend.
		expect(at(layout, 't1').x).toBe(at(layout, 'feature-1').x);
		expect(at(layout, 't2').x).toBe(at(layout, 'feature-1').x);
		expect(at(layout, 't1').y).toBeLessThan(at(layout, 't2').y);
		expect(at(layout, 't2').y).toBeLessThan(at(layout, 't3').y);
		// One edge per parent→child pair.
		expect(edges).toHaveLength(5);
		expect(edges.every((edge) => edge.direction === 'down')).toBe(true);
	});

	it('centres the whole tree horizontally', () => {
		const { layout } = treeLayout(tree);
		const bounds = boundsOf(layout);
		expect(Math.abs(bounds.minX + bounds.maxX)).toBeLessThan(1);
	});
});

describe('graphLayout', () => {
	it('puts prerequisites left of dependents and finds the hub', () => {
		const { layout, edges, hub } = graphLayout(graph);
		expect(hub).toBe('a');
		expect(at(layout, 'a').x).toBeLessThan(at(layout, 'b').x);
		expect(at(layout, 'b').x).toBeLessThan(at(layout, 'd').x);
		expect(edges).toHaveLength(3);
		expect(edges.filter((edge) => edge.hot)).toHaveLength(2);
		expect(edges.every((edge) => edge.direction === 'right')).toBe(true);
	});

	it('centres the graph on the origin', () => {
		const { layout } = graphLayout(graph);
		const bounds = boundsOf(layout);
		expect(Math.abs(bounds.minX + bounds.maxX)).toBeLessThan(1);
		expect(Math.abs(bounds.minY + bounds.maxY)).toBeLessThan(1);
	});
});

describe('timelineLayout', () => {
	const today = new Date('2026-09-02T00:00:00Z');

	it('orders bars by start, packs overlaps into rows, and puts today at x = 0', () => {
		const { layout, labels, hasBars } = timelineLayout(gantt, today);
		expect(hasBars).toBe(true);
		expect(at(layout, 'early').x).toBeLessThan(at(layout, 'late').x);
		expect(at(layout, 'early').x).toBeLessThan(0);
		expect(at(layout, 'late').x).toBeGreaterThan(0);
		// `overlap` starts while `early` is still running, so it takes row 1.
		expect(at(layout, 'early').y).toBe(0);
		expect(at(layout, 'overlap').y).toBe(CARD_HEIGHT + 10);
		// `late` starts after both ended, so row 0 is free again.
		expect(at(layout, 'late').y).toBe(0);
		const todayLabel = labels.find((label) => label.text === 'today');
		expect(todayLabel?.x).toBe(0);
		expect(labels.map((label) => label.text)).toContain('Aug 2026');
		expect(labels.map((label) => label.text)).toContain('Sep 2026');
	});

	it('reports no bars for an empty gantt', () => {
		const { layout, hasBars } = timelineLayout({ ...gantt, bars: [] }, today);
		expect(hasBars).toBe(false);
		expect(layout.size).toBe(0);
	});
});

describe('parkMissing and boundsOf', () => {
	it('parks every id the layout lacks out of sight, and bounds ignore parked cards', () => {
		const layout = parkMissing(new Map([['a', { x: 10, y: 20, z: 0, opacity: 1 }]]), ['a', 'b']);
		expect(at(layout, 'b').opacity).toBe(0);
		const bounds = boundsOf(layout);
		expect(bounds).toEqual({
			minX: 10 - CARD_WIDTH / 2,
			maxX: 10 + CARD_WIDTH / 2,
			minY: 20 - CARD_HEIGHT / 2,
			maxY: 20 + CARD_HEIGHT / 2
		});
	});

	it('returns an empty box for a layout with nothing visible', () => {
		expect(boundsOf(new Map())).toEqual({ minX: 0, maxX: 0, minY: 0, maxY: 0 });
	});
});

describe('edgePath', () => {
	it('starts a downward edge at the bottom of the parent and ends at the top of the child', () => {
		const path = edgePath({
			from: { x: 0, y: 0, z: 0, opacity: 1 },
			to: { x: 100, y: 200, z: 0, opacity: 1 },
			direction: 'down',
			hot: false
		});
		expect(path.startsWith(`M0.0,${(CARD_HEIGHT / 2).toFixed(1)}`)).toBe(true);
		expect(path.endsWith(`100.0,${(200 - CARD_HEIGHT / 2).toFixed(1)}`)).toBe(true);
	});
});
