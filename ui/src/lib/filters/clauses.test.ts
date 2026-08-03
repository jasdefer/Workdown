import { describe, it, expect } from 'vitest';
import {
	clauseToRow,
	clausesEqual,
	clausesToRows,
	isMultiValueOperator,
	isRowComplete,
	operatorLabel,
	rowsToClauses,
	rowToClause,
	withOperator,
	type GuidedRow,
	type Row
} from './clauses';
import type { Clause } from '$lib/api/generated/Clause';

function guided(partial: Partial<Row> & { localId: number }): Row {
	return {
		kind: 'comparison',
		field: '',
		operator: '',
		value: null,
		values: [],
		...partial
	} as Row;
}

describe('operatorLabel', () => {
	it('renders a hybrid word + symbol label', () => {
		expect(operatorLabel('equal')).toBe('is (=)');
		expect(operatorLabel('greater_or_equal')).toBe('at least (≥)');
		expect(operatorLabel('is_not_set')).toBe('is empty');
	});

	it('labels the membership operators in words', () => {
		expect(operatorLabel('in')).toBe('is any of');
		expect(operatorLabel('not_in')).toBe('is none of');
	});
});

describe('isMultiValueOperator', () => {
	it('is the membership operators, not equal', () => {
		expect(isMultiValueOperator('in')).toBe(true);
		expect(isMultiValueOperator('not_in')).toBe(true);
		// `=` means exactly "equals" now — a comma in its value is data.
		expect(isMultiValueOperator('equal')).toBe(false);
		expect(isMultiValueOperator('not_equal')).toBe(false);
	});
});

describe('isRowComplete', () => {
	it('requires a non-empty raw clause', () => {
		expect(isRowComplete({ localId: 1, kind: 'raw', raw: '  ' })).toBe(false);
		expect(isRowComplete({ localId: 1, kind: 'raw', raw: 'status=open' })).toBe(true);
	});

	it('requires field, operator, and value for a comparison', () => {
		expect(isRowComplete(guided({ localId: 1 }))).toBe(false);
		expect(isRowComplete(guided({ localId: 1, field: 'status' }))).toBe(false);
		expect(isRowComplete(guided({ localId: 1, field: 'status', operator: 'equal' }))).toBe(false);
		expect(
			isRowComplete(guided({ localId: 1, field: 'status', operator: 'equal', value: 'open' }))
		).toBe(true);
	});

	it('treats presence operators as complete without a value', () => {
		expect(isRowComplete(guided({ localId: 1, field: 'assignee', operator: 'is_set' }))).toBe(true);
		expect(isRowComplete(guided({ localId: 1, field: 'assignee', operator: 'is_not_set' }))).toBe(
			true
		);
	});

	it('requires at least one member for a membership operator', () => {
		expect(isRowComplete(guided({ localId: 1, field: 'status', operator: 'in' }))).toBe(false);
		expect(
			isRowComplete(guided({ localId: 1, field: 'status', operator: 'in', values: ['open'] }))
		).toBe(true);
		// The scalar slot doesn't make a membership row complete.
		expect(
			isRowComplete(guided({ localId: 1, field: 'status', operator: 'not_in', value: 'open' }))
		).toBe(false);
	});
});

describe('rowToClause', () => {
	it('returns null for an incomplete row', () => {
		expect(rowToClause(guided({ localId: 1, field: 'status' }))).toBeNull();
	});

	it('builds a comparison clause, dropping the value for presence ops', () => {
		expect(
			rowToClause(guided({ localId: 1, field: 'status', operator: 'equal', value: 'open' }))
		).toEqual({
			kind: 'comparison',
			field: 'status',
			operator: 'equal',
			value: 'open',
			values: []
		});
		expect(
			rowToClause(guided({ localId: 1, field: 'assignee', operator: 'is_set', value: 'ignored' }))
		).toEqual({
			kind: 'comparison',
			field: 'assignee',
			operator: 'is_set',
			value: null,
			values: []
		});
	});

	/// The operand slots are mutually exclusive, which is the invariant the
	/// server rejects a request for violating.
	it('sends members in values and nulls the scalar for membership', () => {
		expect(
			rowToClause(
				guided({
					localId: 1,
					field: 'status',
					operator: 'in',
					value: 'stale',
					values: ['open', 'in_progress']
				})
			)
		).toEqual({
			kind: 'comparison',
			field: 'status',
			operator: 'in',
			value: null,
			values: ['open', 'in_progress']
		});
	});

	it('trims a raw clause', () => {
		expect(rowToClause({ localId: 1, kind: 'raw', raw: '  status=open  ' })).toEqual({
			kind: 'raw',
			raw: 'status=open'
		});
	});
});

describe('withOperator', () => {
	function row(partial: Partial<GuidedRow>): GuidedRow {
		return {
			localId: 1,
			kind: 'comparison',
			field: 'status',
			operator: 'equal',
			value: null,
			values: [],
			...partial
		};
	}

	it('promotes a scalar to a one-member list', () => {
		expect(withOperator(row({ operator: 'equal', value: 'open' }), 'in')).toMatchObject({
			operator: 'in',
			value: null,
			values: ['open']
		});
	});

	it('demotes a list to its first member', () => {
		expect(withOperator(row({ operator: 'in', values: ['open', 'done'] }), 'equal')).toMatchObject({
			operator: 'equal',
			value: 'open',
			values: []
		});
	});

	it('keeps the members when switching between the two list operators', () => {
		expect(withOperator(row({ operator: 'in', values: ['open'] }), 'not_in')).toMatchObject({
			operator: 'not_in',
			values: ['open']
		});
	});

	it('clears both slots for a presence operator', () => {
		expect(withOperator(row({ operator: 'in', values: ['open'] }), 'is_set')).toMatchObject({
			operator: 'is_set',
			value: null,
			values: []
		});
	});
});

describe('rowsToClauses', () => {
	it('keeps complete rows and drops the half-built ones', () => {
		const rows: Row[] = [
			guided({ localId: 1, field: 'status', operator: 'equal', value: 'open' }),
			guided({ localId: 2, field: 'points' }), // incomplete → dropped
			guided({ localId: 3, field: 'type', operator: 'in' }), // no members → dropped
			{ localId: 4, kind: 'raw', raw: 'title~fix' }
		];
		expect(rowsToClauses(rows)).toEqual([
			{ kind: 'comparison', field: 'status', operator: 'equal', value: 'open', values: [] },
			{ kind: 'raw', raw: 'title~fix' }
		]);
	});
});

describe('clause ↔ row round-trip', () => {
	it('seeds rows from clauses and back', () => {
		const clauses: Clause[] = [
			{ kind: 'comparison', field: 'status', operator: 'equal', value: 'open', values: [] },
			{
				kind: 'comparison',
				field: 'type',
				operator: 'in',
				value: null,
				values: ['milestone', 'epic']
			},
			{ kind: 'comparison', field: 'status', operator: 'not_in', value: null, values: ['done'] },
			{ kind: 'raw', raw: 'parent.status=done' }
		];
		let id = 0;
		const rows = clausesToRows(clauses, () => (id += 1));
		expect(rows.map((row) => row.localId)).toEqual([1, 2, 3, 4]);
		expect(rowsToClauses(rows)).toEqual(clauses);
	});

	it('preserves a presence clause with a null value', () => {
		const clause: Clause = {
			kind: 'comparison',
			field: 'assignee',
			operator: 'is_not_set',
			value: null,
			values: []
		};
		const row = clauseToRow(clause, 1);
		expect(rowToClause(row)).toEqual(clause);
	});
});

describe('clausesEqual', () => {
	it('detects an unsaved change', () => {
		const saved: Clause[] = [
			{ kind: 'comparison', field: 'status', operator: 'equal', value: 'open', values: [] }
		];
		const same: Clause[] = [
			{ kind: 'comparison', field: 'status', operator: 'equal', value: 'open', values: [] }
		];
		const changed: Clause[] = [
			{ kind: 'comparison', field: 'status', operator: 'equal', value: 'done', values: [] }
		];
		expect(clausesEqual(saved, same)).toBe(true);
		expect(clausesEqual(saved, changed)).toBe(false);
	});

	it('detects a changed member list', () => {
		const saved: Clause[] = [
			{ kind: 'comparison', field: 'type', operator: 'in', value: null, values: ['epic'] }
		];
		const changed: Clause[] = [
			{
				kind: 'comparison',
				field: 'type',
				operator: 'in',
				value: null,
				values: ['epic', 'milestone']
			}
		];
		expect(clausesEqual(saved, changed)).toBe(false);
	});
});
