import { describe, it, expect } from 'vitest';
import {
	clauseToRow,
	clausesEqual,
	clausesToRows,
	isRowComplete,
	operatorLabel,
	rowsToClauses,
	rowToClause,
	type Row
} from './clauses';
import type { Clause } from '$lib/api/generated/Clause';

function guided(partial: Partial<Row> & { localId: number }): Row {
	return {
		kind: 'comparison',
		field: '',
		operator: '',
		value: null,
		...partial
	} as Row;
}

describe('operatorLabel', () => {
	it('renders a hybrid word + symbol label', () => {
		expect(operatorLabel('equal')).toBe('is (=)');
		expect(operatorLabel('greater_or_equal')).toBe('at least (≥)');
		expect(operatorLabel('is_not_set')).toBe('is empty');
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
});

describe('rowToClause', () => {
	it('returns null for an incomplete row', () => {
		expect(rowToClause(guided({ localId: 1, field: 'status' }))).toBeNull();
	});

	it('builds a comparison clause, dropping the value for presence ops', () => {
		expect(
			rowToClause(guided({ localId: 1, field: 'status', operator: 'equal', value: 'open' }))
		).toEqual({ kind: 'comparison', field: 'status', operator: 'equal', value: 'open' });
		expect(
			rowToClause(guided({ localId: 1, field: 'assignee', operator: 'is_set', value: 'ignored' }))
		).toEqual({ kind: 'comparison', field: 'assignee', operator: 'is_set', value: null });
	});

	it('trims a raw clause', () => {
		expect(rowToClause({ localId: 1, kind: 'raw', raw: '  status=open  ' })).toEqual({
			kind: 'raw',
			raw: 'status=open'
		});
	});
});

describe('rowsToClauses', () => {
	it('keeps complete rows and drops the half-built ones', () => {
		const rows: Row[] = [
			guided({ localId: 1, field: 'status', operator: 'equal', value: 'open' }),
			guided({ localId: 2, field: 'points' }), // incomplete → dropped
			{ localId: 3, kind: 'raw', raw: 'title~fix' }
		];
		expect(rowsToClauses(rows)).toEqual([
			{ kind: 'comparison', field: 'status', operator: 'equal', value: 'open' },
			{ kind: 'raw', raw: 'title~fix' }
		]);
	});
});

describe('clause ↔ row round-trip', () => {
	it('seeds rows from clauses and back', () => {
		const clauses: Clause[] = [
			{ kind: 'comparison', field: 'status', operator: 'equal', value: 'open,in_progress' },
			{ kind: 'raw', raw: 'parent.status=done' }
		];
		let id = 0;
		const rows = clausesToRows(clauses, () => (id += 1));
		expect(rows.map((row) => row.localId)).toEqual([1, 2]);
		expect(rowsToClauses(rows)).toEqual(clauses);
	});

	it('preserves a presence clause with a null value', () => {
		const clause: Clause = {
			kind: 'comparison',
			field: 'assignee',
			operator: 'is_not_set',
			value: null
		};
		const row = clauseToRow(clause, 1);
		expect(rowToClause(row)).toEqual(clause);
	});
});

describe('clausesEqual', () => {
	it('detects an unsaved change', () => {
		const saved: Clause[] = [
			{ kind: 'comparison', field: 'status', operator: 'equal', value: 'open' }
		];
		const same: Clause[] = [
			{ kind: 'comparison', field: 'status', operator: 'equal', value: 'open' }
		];
		const changed: Clause[] = [
			{ kind: 'comparison', field: 'status', operator: 'equal', value: 'done' }
		];
		expect(clausesEqual(saved, same)).toBe(true);
		expect(clausesEqual(saved, changed)).toBe(false);
	});
});
