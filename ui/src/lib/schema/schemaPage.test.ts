import { describe, it, expect } from 'vitest';
import type { ApiResult } from '$lib/api/client';
import type { DerivedBlocks } from '$lib/api/generated/DerivedBlocks';
import type { SchemaDefinitionData } from '$lib/api/generated/SchemaDefinitionData';
import {
	fillMechanisms,
	firstLine,
	idFirst,
	loadFailure,
	settingsLine,
	typeSummary
} from './schemaPage';

const plain: DerivedBlocks = { compute: null, when: null, pull: null, aggregate: null };

describe('idFirst', () => {
	it('moves id to the top wherever the file declares it', () => {
		const ordered = idFirst([{ name: 'title' }, { name: 'status' }, { name: 'id' }]);
		expect(ordered.map((field) => field.name)).toEqual(['id', 'title', 'status']);
	});

	it('keeps declaration order when id is already first', () => {
		const fields = [{ name: 'id' }, { name: 'title' }];
		expect(idFirst(fields)).toBe(fields);
	});

	it('leaves a schema without an id field unchanged', () => {
		const fields = [{ name: 'title' }, { name: 'status' }];
		expect(idFirst(fields)).toBe(fields);
	});
});

describe('fillMechanisms', () => {
	it('is empty for a field written by hand', () => {
		expect(fillMechanisms({ derived: plain })).toEqual([]);
	});

	it('names each fill mechanism the field declares, in a fixed order', () => {
		const derived: DerivedBlocks = {
			compute: 'estimate * 2',
			when: '- if: ...',
			pull: 'parent.owner',
			aggregate: 'sum'
		};
		expect(fillMechanisms({ derived })).toEqual([
			'computed',
			'conditional',
			'pulled',
			'aggregated'
		]);
	});

	it('lists the compute-plus-aggregate combination as two', () => {
		const derived: DerivedBlocks = { ...plain, compute: 'a + b', aggregate: 'sum' };
		expect(fillMechanisms({ derived })).toEqual(['computed', 'aggregated']);
	});
});

describe('settingsLine', () => {
	it('is the shape summary alone without a resource', () => {
		expect(settingsLine({ shape: { kind: 'values', values: ['a', 'b'] }, resource: null })).toBe(
			'a · b'
		);
	});

	it('appends the resource list after the shape summary', () => {
		expect(settingsLine({ shape: { kind: 'text', pattern: '^[a-z]+$' }, resource: 'people' })).toBe(
			'pattern ^[a-z]+$ · from people'
		);
	});

	it('is the resource alone when the shape has nothing to say', () => {
		expect(settingsLine({ shape: { kind: 'text', pattern: null }, resource: 'people' })).toBe(
			'from people'
		);
	});

	it('is null for a plain scalar without a resource', () => {
		expect(settingsLine({ shape: { kind: 'scalar' }, resource: null })).toBeNull();
	});
});

describe('typeSummary', () => {
	it('has nothing to say for a plain scalar', () => {
		expect(typeSummary({ kind: 'scalar' })).toBeNull();
	});

	it('phrases a numeric range by which bounds are set', () => {
		expect(typeSummary({ kind: 'numeric', min: 0, max: 100 })).toBe('0 to 100');
		expect(typeSummary({ kind: 'numeric', min: 1.5, max: null })).toBe('at least 1.5');
		expect(typeSummary({ kind: 'numeric', min: null, max: 10 })).toBe('at most 10');
		expect(typeSummary({ kind: 'numeric', min: null, max: null })).toBeNull();
	});

	it('renders duration bounds as suffix shorthand', () => {
		// 40 hours, in the compact form every table and chart uses. The
		// payload types the seconds as bigint, JSON delivers a number — both
		// are accepted.
		expect(typeSummary({ kind: 'duration', min_seconds: null, max_seconds: 144000n })).toBe(
			'at most 1d 16h'
		);
		expect(
			typeSummary({ kind: 'duration', min_seconds: 900 as unknown as bigint, max_seconds: null })
		).toBe('at least 15min');
	});

	it('shows the pattern of a text field, nothing without one', () => {
		expect(typeSummary({ kind: 'text', pattern: '^[a-z-]+$' })).toBe('pattern ^[a-z-]+$');
		expect(typeSummary({ kind: 'text', pattern: null })).toBeNull();
	});

	it('lists the values of a choice in order', () => {
		expect(typeSummary({ kind: 'values', values: ['to_do', 'in_progress', 'done'] })).toBe(
			'to_do · in_progress · done'
		);
		expect(typeSummary({ kind: 'values', values: [] })).toBeNull();
	});

	it('names a relation by its inverse and cycle rule', () => {
		expect(typeSummary({ kind: 'relation', allow_cycles: false, inverse: 'children' })).toBe(
			'inverse children · no cycles'
		);
		expect(typeSummary({ kind: 'relation', allow_cycles: true, inverse: null })).toBe(
			'cycles allowed'
		);
		expect(typeSummary({ kind: 'relation', allow_cycles: null, inverse: null })).toBeNull();
	});
});

describe('firstLine', () => {
	it('passes a one-line description through trimmed', () => {
		expect(firstLine('  Display name.  ')).toBe('Display name.');
	});

	it('keeps only the first non-blank line of a longer description', () => {
		expect(firstLine('\nWhat it is.\nMore detail below.')).toBe('What it is.');
	});

	it('is null for no description or a blank one', () => {
		expect(firstLine(null)).toBeNull();
		expect(firstLine('   \n  ')).toBeNull();
	});
});

describe('loadFailure', () => {
	function result(
		partial: Partial<ApiResult<SchemaDefinitionData>>
	): ApiResult<SchemaDefinitionData> {
		return { diagnostics: [], status: 422, ...partial };
	}

	it('takes the path and message from the load diagnostic', () => {
		const failure = loadFailure(
			result({
				diagnostics: [
					{
						severity: 'error',
						message: 'fields.status: missing `values`',
						scope: 'file',
						source_path: '.workdown/schema.yaml',
						type: 'read_error',
						detail: 'fields.status: missing `values`'
					}
				]
			})
		);
		expect(failure).toEqual({
			path: '.workdown/schema.yaml',
			detail: 'fields.status: missing `values`'
		});
	});

	it('falls back to the request error when no diagnostic names the file', () => {
		expect(loadFailure(result({ status: 0, error: 'Failed to fetch' }))).toEqual({
			path: null,
			detail: 'Failed to fetch'
		});
	});

	it('has a generic line when the reply carried neither', () => {
		expect(loadFailure(result({ status: 500 })).detail).toBe('The schema could not be loaded.');
	});
});
