// Filter-editor clause helpers.
//
// The editor works in terms of local, editable `Row`s; the wire speaks
// `Clause` (a guided `Condition` or a raw string). These pure functions
// convert between them and decide when a row is "complete" enough to
// preview or save. Building/parsing clause *syntax* stays in the Rust core
// (see `query::clause`); here we only shuffle the structured shape around,
// so nothing in this file needs to know that `equal` renders as `=`.

import type { Clause } from '$lib/api/generated/Clause';
import type { Operator } from '$lib/api/generated/Operator';

/** A guided condition row: field → operator → value pickers. */
export interface GuidedRow {
	/** Stable key for `{#each}`; never crosses the wire. */
	localId: number;
	kind: 'comparison';
	/** Empty until a field is picked. */
	field: string;
	/** Empty until an operator is picked. */
	operator: Operator | '';
	/** `null` for presence operators (`is set` / `is empty`) and for the
	 * list-valued operators, which carry their members in `values`. */
	value: string | null;
	/** Members for `in` / `not in`; empty for every other operator. */
	values: string[];
}

/** A raw clause row — the escape hatch, edited as plain text. */
export interface RawRow {
	localId: number;
	kind: 'raw';
	raw: string;
}

export type Row = GuidedRow | RawRow;

// ── Operator labels (hybrid: word + symbol hint) ─────────────────────

const OPERATOR_LABELS: Record<Operator, string> = {
	equal: 'is (=)',
	not_equal: 'is not (≠)',
	greater_than: 'greater than (>)',
	less_than: 'less than (<)',
	greater_or_equal: 'at least (≥)',
	less_or_equal: 'at most (≤)',
	contains: 'contains (~)',
	matches: 'matches regex',
	is_set: 'is set',
	is_not_set: 'is empty',
	in: 'is any of',
	not_in: 'is none of'
};

export function operatorLabel(operator: Operator): string {
	return OPERATOR_LABELS[operator];
}

/** Presence operators take no value. */
export function isPresenceOperator(operator: Operator | ''): boolean {
	return operator === 'is_set' || operator === 'is_not_set';
}

/**
 * List-valued operators read their members from `values` and render a
 * multi-select. `equal` is single-value — it means exactly "equals", commas
 * included — so membership is its own operator now.
 */
export function isMultiValueOperator(operator: Operator | ''): boolean {
	return operator === 'in' || operator === 'not_in';
}

// ── Completeness ─────────────────────────────────────────────────────

/**
 * Whether a row is filled in enough to preview/save. Half-built guided
 * rows (no field, no operator, no value picked) are skipped so the server
 * never sees `status=` or an empty `in` list.
 */
export function isRowComplete(row: Row): boolean {
	if (row.kind === 'raw') return row.raw.trim() !== '';
	if (row.field === '' || row.operator === '') return false;
	if (isPresenceOperator(row.operator)) return true;
	if (isMultiValueOperator(row.operator)) return row.values.length > 0;
	return row.value !== null && row.value.trim() !== '';
}

// ── Row ↔ Clause ─────────────────────────────────────────────────────

/**
 * Convert a complete row to its wire clause, or `null` if incomplete.
 *
 * The operand slots are mutually exclusive and chosen by the operator, which
 * is the invariant the server enforces: a list operator sends `values` and a
 * null `value`, everything else the reverse.
 */
export function rowToClause(row: Row): Clause | null {
	if (!isRowComplete(row)) return null;
	if (row.kind === 'raw') return { kind: 'raw', raw: row.raw.trim() };
	const operator = row.operator as Operator;
	if (isMultiValueOperator(operator)) {
		return { kind: 'comparison', field: row.field, operator, value: null, values: row.values };
	}
	return {
		kind: 'comparison',
		field: row.field,
		operator,
		value: isPresenceOperator(operator) ? null : row.value,
		values: []
	};
}

/** All complete rows, as wire clauses — what preview and save send. */
export function rowsToClauses(rows: Row[]): Clause[] {
	return rows.map(rowToClause).filter((clause): clause is Clause => clause !== null);
}

/** Seed a row from a clause returned by the server. */
export function clauseToRow(clause: Clause, localId: number): Row {
	if (clause.kind === 'raw') return { localId, kind: 'raw', raw: clause.raw };
	return {
		localId,
		kind: 'comparison',
		field: clause.field,
		operator: clause.operator,
		value: clause.value,
		values: clause.values
	};
}

/**
 * Move a row's operand across an operator change, converting between the
 * scalar and list forms instead of dropping what the user picked: the first
 * member becomes the scalar, and a scalar becomes a one-member list.
 *
 * Presence operators take no operand at all, so both slots clear.
 */
export function withOperator(row: GuidedRow, operator: Operator | ''): GuidedRow {
	if (operator === '' || isPresenceOperator(operator)) {
		return { ...row, operator, value: null, values: [] };
	}
	if (isMultiValueOperator(operator)) {
		const carried = row.values.length > 0 ? row.values : row.value ? [row.value] : [];
		return { ...row, operator, value: null, values: carried };
	}
	const carried = row.value ?? row.values[0] ?? '';
	return { ...row, operator, value: carried, values: [] };
}

/** Seed the editor's rows from a persisted, decomposed filter. */
export function clausesToRows(clauses: Clause[], nextId: () => number): Row[] {
	return clauses.map((clause) => clauseToRow(clause, nextId()));
}

/**
 * Structural equality of two clause lists — drives the "unsaved" state.
 * Order-sensitive, which is what we want (reordering is a change).
 */
export function clausesEqual(a: Clause[], b: Clause[]): boolean {
	return JSON.stringify(a) === JSON.stringify(b);
}
