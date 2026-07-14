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
	/** `null` for presence operators (`is set` / `is empty`). */
	value: string | null;
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
	is_not_set: 'is empty'
};

export function operatorLabel(operator: Operator): string {
	return OPERATOR_LABELS[operator];
}

/** Presence operators take no value. */
export function isPresenceOperator(operator: Operator | ''): boolean {
	return operator === 'is_set' || operator === 'is_not_set';
}

/**
 * Multi-value (IN) is only meaningful for `equal` — the grammar treats a
 * comma list as "any of" only there. The UI offers a multi-select for
 * `equal` on choice-like fields; every other operator is single-value.
 */
export function isMultiValueOperator(operator: Operator | ''): boolean {
	return operator === 'equal';
}

// ── Completeness ─────────────────────────────────────────────────────

/**
 * Whether a row is filled in enough to preview/save. Half-built guided
 * rows (no field, no operator, empty value) are skipped so the server
 * never sees `status=`.
 */
export function isRowComplete(row: Row): boolean {
	if (row.kind === 'raw') return row.raw.trim() !== '';
	if (row.field === '' || row.operator === '') return false;
	if (isPresenceOperator(row.operator)) return true;
	return row.value !== null && row.value.trim() !== '';
}

// ── Row ↔ Clause ─────────────────────────────────────────────────────

/** Convert a complete row to its wire clause, or `null` if incomplete. */
export function rowToClause(row: Row): Clause | null {
	if (!isRowComplete(row)) return null;
	if (row.kind === 'raw') return { kind: 'raw', raw: row.raw.trim() };
	const operator = row.operator as Operator;
	return {
		kind: 'comparison',
		field: row.field,
		operator,
		value: isPresenceOperator(operator) ? null : row.value
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
		value: clause.value
	};
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
