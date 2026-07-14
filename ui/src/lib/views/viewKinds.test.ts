import { describe, it, expect } from 'vitest';
import { VIEW_KIND_CONTROLS, VIEW_KINDS, fieldFits, isDefinitionComplete } from './viewKinds';

describe('VIEW_KINDS', () => {
	it('lists all 13 kinds, each with a control spec', () => {
		expect(VIEW_KINDS).toHaveLength(13);
		for (const kind of VIEW_KINDS) {
			expect(VIEW_KIND_CONTROLS[kind].length).toBeGreaterThan(0);
		}
	});
});

describe('fieldFits', () => {
	it('accepts any field when the list is empty', () => {
		expect(fieldFits('date', [])).toBe(true);
	});

	it('constrains to the listed types otherwise', () => {
		expect(fieldFits('choice', ['choice', 'string'])).toBe(true);
		expect(fieldFits('date', ['choice', 'string'])).toBe(false);
	});
});

describe('isDefinitionComplete', () => {
	it('requires the mandatory field slot for a board', () => {
		expect(isDefinitionComplete('board', {})).toBe(false);
		expect(isDefinitionComplete('board', { field: 'status' })).toBe(true);
	});

	it('ignores optional slots (tree columns)', () => {
		expect(isDefinitionComplete('tree', { field: 'parent' })).toBe(true);
	});

	it('requires a non-empty column list for a table', () => {
		expect(isDefinitionComplete('table', { columns: [] })).toBe(false);
		expect(isDefinitionComplete('table', { columns: ['id'] })).toBe(true);
	});

	it('requires gantt start plus end or duration', () => {
		expect(isDefinitionComplete('gantt', { start: 'start_date' })).toBe(false);
		expect(isDefinitionComplete('gantt', { start: 'start_date', end: 'end_date' })).toBe(true);
		expect(isDefinitionComplete('gantt', { start: 'start_date', duration: 'estimate' })).toBe(true);
	});

	it('requires at least one metric row with an aggregate', () => {
		expect(isDefinitionComplete('metric', { metrics: [] })).toBe(false);
		expect(isDefinitionComplete('metric', { metrics: [{ label: 'x' }] })).toBe(false);
		expect(isDefinitionComplete('metric', { metrics: [{ aggregate: 'count' }] })).toBe(true);
	});

	it('requires group_by and aggregate for a bar chart, value optional', () => {
		expect(isDefinitionComplete('bar_chart', { group_by: 'status' })).toBe(false);
		expect(isDefinitionComplete('bar_chart', { group_by: 'status', aggregate: 'count' })).toBe(
			true
		);
	});
});
