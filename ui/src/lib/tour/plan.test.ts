import { describe, expect, it } from 'vitest';
import type { ViewSummary } from '$lib/api/generated/ViewSummary';
import { planTour, plannedViewIds } from './plan';

const view = (id: string, kind: ViewSummary['kind']): ViewSummary => ({ id, title: null, kind });

describe('planTour', () => {
	it('takes the first view of each kind and lands on the first view overall', () => {
		const plan = planTour([
			view('board-a', 'board'),
			view('tree-a', 'tree'),
			view('metric-a', 'metric'),
			view('tree-b', 'tree'),
			view('graph-a', 'graph'),
			view('gantt-a', 'gantt'),
			view('gantt-b', 'gantt')
		]);
		expect(plan).toEqual({
			metric: 'metric-a',
			tree: 'tree-a',
			boards: ['board-a'],
			graph: 'graph-a',
			gantt: 'gantt-a',
			landing: 'board-a'
		});
	});

	it('keeps at most two boards, in views.yaml order', () => {
		const plan = planTour([view('one', 'board'), view('two', 'board'), view('three', 'board')]);
		expect(plan.boards).toEqual(['one', 'two']);
	});

	it('leaves every slot empty for a project without views', () => {
		const plan = planTour([]);
		expect(plan).toEqual({
			metric: null,
			tree: null,
			boards: [],
			graph: null,
			gantt: null,
			landing: null
		});
		expect(plannedViewIds(plan)).toEqual([]);
	});

	it('skips the timeline when no gantt view exists', () => {
		expect(planTour([view('board', 'board'), view('table', 'table')]).gantt).toBeNull();
	});
});

describe('plannedViewIds', () => {
	it('lists each referenced view once, landing included', () => {
		const plan = planTour([view('board', 'board'), view('tree', 'tree'), view('stats', 'metric')]);
		expect(plannedViewIds(plan)).toEqual(['stats', 'tree', 'board']);
	});

	it('does not repeat the landing view when it also feeds a scene', () => {
		const ids = plannedViewIds(planTour([view('board', 'board')]));
		expect(ids).toEqual(['board']);
	});
});
