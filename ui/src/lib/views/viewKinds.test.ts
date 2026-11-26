import { describe, it, expect } from 'vitest';
import {
	VIEW_KIND_CONTROLS,
	VIEW_KINDS,
	fieldFits,
	isDefinitionComplete,
	slotOptions
} from './viewKinds';

describe('VIEW_KINDS', () => {
	// Completeness is a compile-time matter now: the picker list is derived
	// from a `Record<ViewType, string>`, so a missing kind cannot get past
	// `npm run check`. What a test can still add is that every listed kind
	// actually has inputs to offer, which no type expresses.
	it('gives every kind at least one control', () => {
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

describe('slotOptions', () => {
	const fields = [
		{ name: 'status', field_type: 'choice', inverse: null },
		{ name: 'depends_on', field_type: 'links', inverse: 'dependents' },
		{ name: 'related_to', field_type: 'links', inverse: null }
	] as const;

	it('offers the fields of an accepted type, in schema order', () => {
		expect(slotOptions(fields, ['link', 'links']).map((option) => option.value)).toEqual([
			'depends_on',
			'related_to'
		]);
	});

	it('adds a declared inverse name after its relation when the slot takes them', () => {
		expect(slotOptions(fields, ['link', 'links'], true)).toEqual([
			{ value: 'depends_on', label: 'depends_on' },
			{ value: 'dependents', label: 'dependents (depends_on, reversed)' },
			{ value: 'related_to', label: 'related_to' }
		]);
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

	it('accepts a table without columns (fields role falls back)', () => {
		expect(isDefinitionComplete('table', {})).toBe(true);
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
