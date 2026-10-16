// The rule editor's working copy and what it is made from and into:
// the draft a panel opens with (from a served rule, or empty for a new
// one), the rows as the form edits them, the write body a save sends,
// and what greys out Save. Pure so every input shape can be unit-tested
// without a DOM; the panel component holds the draft and calls these.
//
// A rule reaches the browser as rows (`RuleShape`), one field reference
// and one test per row, built by the server from the parsed rule and
// written back by it; the YAML grammar stays in Rust. What this module
// adds is the form's view of a reference — an anchor (this item, or a
// relation) and a field — and which tests a reference can take: by
// what it points at (one value, many, the related items themselves),
// and by its target type through the lists the definition payload
// serves from the rule engine (`rule_value_test_types`,
// `rule_comparable_types`). Nothing here knows a type's rules.

import type { FieldDefinitionData } from '$lib/api/generated/FieldDefinitionData';
import type { FieldType } from '$lib/api/generated/FieldType';
import type { FieldValue } from '$lib/api/generated/FieldValue';
import type { MatchRow } from '$lib/api/generated/MatchRow';
import type { RequireRow } from '$lib/api/generated/RequireRow';
import type { RequireTest } from '$lib/api/generated/RequireTest';
import type { RuleData } from '$lib/api/generated/RuleData';
import type { RuleDefinitionWrite } from '$lib/api/generated/RuleDefinitionWrite';
import type { RuleShape } from '$lib/api/generated/RuleShape';
import type { ScalarTest } from '$lib/api/generated/ScalarTest';
import type { ScalarValue } from '$lib/api/generated/ScalarValue';
import type { SchemaDefinitionData } from '$lib/api/generated/SchemaDefinitionData';
import type { Severity } from '$lib/api/generated/Severity';
import { specOf, type ValueSpec } from '$lib/values/valueSpec';

/** The panel's working copy of one rule. */
export interface RuleDraft {
	/** As typed; judged by `ruleProblem` and, on save, the server. */
	name: string;
	severity: Severity;
	/** As typed; blank writes as no description. */
	description: string;
	shape: RuleShape;
}

/** The draft a panel opens with for an existing rule whose body has rows. */
export function initialRuleDraft(
	rule: Pick<RuleData, 'name' | 'severity' | 'description'>,
	shape: RuleShape
): RuleDraft {
	return {
		name: rule.name,
		severity: rule.severity,
		description: rule.description ?? '',
		shape
	};
}

/** The empty draft for a new rule. */
export function newRuleDraft(): RuleDraft {
	return {
		name: '',
		severity: 'error',
		description: '',
		shape: { match_rows: [], require_rows: [], count: null }
	};
}

/** The body a save or a preview sends: the draft with blank text as "none". */
export function ruleWriteBody(draft: RuleDraft): RuleDefinitionWrite {
	const description = draft.description.trim();
	return {
		name: draft.name.trim(),
		description: description === '' ? null : description,
		severity: draft.severity,
		shape: draft.shape
	};
}

// ── References ────────────────────────────────────────────────────────

/**
 * A relation a reference can go through: a `link` or `links` field, or
 * the inverse a `link`/`links` field declares. `many` says whether it
 * leads to several items — a `links` field or any inverse — which is
 * what decides whether a quantifier applies.
 */
export interface Anchor {
	name: string;
	many: boolean;
}

/** The relations the schema offers as anchors, in declaration order, inverses beside their field. */
export function anchorsOf(
	fields: Pick<FieldDefinitionData, 'name' | 'field_type' | 'shape'>[]
): Anchor[] {
	const anchors: Anchor[] = [];
	for (const field of fields) {
		if (field.shape.kind !== 'relation') continue;
		anchors.push({ name: field.name, many: field.field_type === 'links' });
		if (field.shape.inverse !== null) anchors.push({ name: field.shape.inverse, many: true });
	}
	return anchors;
}

/**
 * A reference as the form edits it: through which relation (`null`
 * for this item) and which field (`null` for the related items
 * themselves, which only a many-valued anchor can mean).
 */
export interface Reference {
	anchor: string | null;
	field: string | null;
}

/**
 * The form's reading of a served reference. `children.status` is the
 * anchor and the field; a bare name is the field on this item when a
 * field has that name, else the anchor on its own (an inverse such as
 * `children`). A bare `links` field is both, and reads as the field: the
 * two spellings write back the same.
 */
export function splitReference(
	reference: string,
	fields: Pick<FieldDefinitionData, 'name'>[],
	anchors: Anchor[]
): Reference {
	const dot = reference.indexOf('.');
	if (dot >= 0) {
		return { anchor: reference.slice(0, dot), field: reference.slice(dot + 1) };
	}
	if (fields.some((field) => field.name === reference)) return { anchor: null, field: reference };
	if (anchors.some((anchor) => anchor.name === reference))
		return { anchor: reference, field: null };
	return { anchor: null, field: reference };
}

/** The served spelling of a reference; `null` when it names nothing yet. */
export function joinReference(reference: Reference): string | null {
	if (reference.anchor === null) return reference.field;
	if (reference.field === null) return reference.anchor;
	return `${reference.anchor}.${reference.field}`;
}

/**
 * What a reference points at, which decides the tests it can take:
 * one value (a field on this item, or on the one item a `link`
 * leads to), one value per related item (`many`), the related items
 * themselves (`many_bare`), or nothing yet.
 */
export type ReferenceKind = 'single' | 'many' | 'many_bare' | 'incomplete';

export function referenceKind(reference: Reference, anchors: Anchor[]): ReferenceKind {
	if (reference.anchor === null) return reference.field === null ? 'incomplete' : 'single';
	const anchor = anchors.find((candidate) => candidate.name === reference.anchor);
	const many = anchor?.many ?? true;
	if (reference.field === null) return many ? 'many_bare' : 'incomplete';
	return many ? 'many' : 'single';
}

/** The field a reference's test is judged against, or `null` for none (bare, incomplete, unknown). */
export function targetField<Field extends Pick<FieldDefinitionData, 'name'>>(
	reference: Reference,
	fields: Field[]
): Field | null {
	if (reference.field === null) return null;
	return fields.find((field) => field.name === reference.field) ?? null;
}

// ── Which tests a reference takes ─────────────────────────────────────

/**
 * What the match test picker offers: the wire's test kinds, with
 * `is_set` split into its two answers so "is set" and "is not set" are
 * picked in one place rather than a kind and then a yes/no.
 */
export type MatchTestChoice = 'equals' | 'one_of' | 'not' | 'is_set' | 'is_not_set';

/**
 * The choices each wire kind spells, in the order the picker lists
 * them. Exhaustive by type: a test kind added in Rust reaches the
 * union and fails the build here until the picker knows it.
 */
const CHOICES_OF_KIND: Record<ScalarTest['kind'], MatchTestChoice[]> = {
	equals: ['equals'],
	one_of: ['one_of'],
	not: ['not'],
	is_set: ['is_set', 'is_not_set']
};

const ALL_CHOICES: MatchTestChoice[] = (
	Object.keys(CHOICES_OF_KIND) as ScalarTest['kind'][]
).flatMap((kind) => CHOICES_OF_KIND[kind]);

/**
 * The match tests a reference can take. A reference to a value takes
 * every test; the related items themselves can only be asked whether
 * there are any. Nothing for a reference that names nothing yet.
 */
export function matchTestChoices(kind: ReferenceKind): MatchTestChoice[] {
	switch (kind) {
		case 'single':
		case 'many':
			return ALL_CHOICES;
		case 'many_bare':
			return CHOICES_OF_KIND.is_set;
		case 'incomplete':
			return [];
	}
}

/** The picker's choice a test shows as. */
export function choiceOfTest(test: ScalarTest): MatchTestChoice {
	if (test.kind === 'is_set') return test.value ? 'is_set' : 'is_not_set';
	return test.kind;
}

/** The test a picked choice stands for, with nothing entered. */
export function testOfChoice(choice: MatchTestChoice): ScalarTest {
	switch (choice) {
		case 'equals':
			return { kind: 'equals', value: '' };
		case 'one_of':
		case 'not':
			return { kind: choice, values: [] };
		case 'is_set':
			return { kind: 'is_set', value: true };
		case 'is_not_set':
			return { kind: 'is_set', value: false };
	}
}

/** Whether a match row on this reference is read through a quantifier. */
export function needsQuantifier(kind: ReferenceKind): boolean {
	return kind === 'many';
}

/** The lists the rule engine serves of which types it tests by value and which it orders. */
export type RuleTables = Pick<
	SchemaDefinitionData,
	'rule_value_test_types' | 'rule_comparable_types'
>;

/**
 * The require tests a reference can take, by what it points at and
 * the target field's type: presence always; a value set where the
 * engine tests the type by value; a comparison where it has an order
 * for the type; a count only on the related items themselves.
 */
export function requireTestKinds(
	kind: ReferenceKind,
	targetType: FieldType | null,
	tables: RuleTables
): RequireTest['kind'][] {
	switch (kind) {
		case 'incomplete':
			return [];
		case 'many_bare':
			return ['min_count', 'max_count'];
		case 'single':
		case 'many': {
			const kinds: RequireTest['kind'][] = ['required', 'forbidden'];
			if (targetType !== null && tables.rule_value_test_types.includes(targetType)) {
				kinds.push('values', 'not');
			}
			if (targetType !== null && tables.rule_comparable_types.includes(targetType)) {
				kinds.push('compare');
			}
			return kinds;
		}
	}
}

/**
 * What a comparison may compare with: the other fields of the same
 * type, and `$today` for a date. The target itself is left out; a
 * field always equals itself.
 */
export function comparableFields(
	fields: Pick<FieldDefinitionData, 'name' | 'field_type'>[],
	target: Pick<FieldDefinitionData, 'name' | 'field_type'>
): string[] {
	const names = fields
		.filter((field) => field.field_type === target.field_type && field.name !== target.name)
		.map((field) => field.name);
	return target.field_type === 'date' ? ['$today', ...names] : names;
}

// ── Values ────────────────────────────────────────────────────────────

/** What the value editor needs to edit one value of `field`; free text when there is no field. */
export function specOfField(
	field: Pick<FieldDefinitionData, 'field_type' | 'shape'> | null
): ValueSpec {
	return field === null ? specOf('string', null) : specOf(field.field_type, field.shape);
}

/**
 * What the value editor needs to edit a list of values of `field` (a
 * `one_of`, a `not`, a `values`): the choice's values as checkboxes,
 * free tags otherwise. The tags are typed as text and read back as the
 * field's type by `scalarsOfValue`.
 */
export function listSpecOfField(
	field: Pick<FieldDefinitionData, 'field_type' | 'shape'> | null
): ValueSpec {
	return field !== null && field.shape.kind === 'values'
		? specOf('multichoice', field.shape)
		: specOf('list', null);
}

/**
 * A count typed into a number input — a `count` bound, a `min_count`
 * or `max_count` — as the whole number it names, or `null` when the
 * text is blank, not a number or negative. What `null` means is the
 * caller's: a cleared bound, or a keystroke to ignore.
 */
export function parseCount(text: string): number | null {
	if (text.trim() === '') return null;
	const parsed = Number.parseInt(text, 10);
	return Number.isNaN(parsed) || parsed < 0 ? null : parsed;
}

/** One value as the editor handed it over, as the wire's scalar; `null` when cleared or not one value. */
export function scalarOfValue(value: FieldValue | null): ScalarValue | null {
	if (value === null || Array.isArray(value)) return null;
	if (typeof value === 'bigint') return Number(value);
	return value;
}

/**
 * The values of a list editor as the wire's scalars, read as the
 * target type: a number for a numeric field when the text is one,
 * `true`/`false` for a boolean, the text otherwise.
 */
export function scalarsOfValue(
	value: FieldValue | null,
	targetType: FieldType | null
): ScalarValue[] {
	if (!Array.isArray(value)) return [];
	return value.map((text) => scalarOfText(text, targetType));
}

function scalarOfText(text: string, targetType: FieldType | null): ScalarValue {
	if (targetType === 'integer' || targetType === 'float') {
		const number = Number(text);
		if (text.trim() !== '' && Number.isFinite(number)) return number;
	}
	if (targetType === 'boolean') {
		if (text === 'true') return true;
		if (text === 'false') return false;
	}
	return text;
}

/** One wire scalar as the editor shows it. */
export function valueOfScalar(value: ScalarValue): FieldValue {
	return value;
}

/** Wire scalars as the list editor shows them: their text. */
export function valueOfScalars(values: ScalarValue[]): FieldValue {
	return values.map((value) => String(value));
}

// ── Rows ──────────────────────────────────────────────────────────────

/** A match row with nothing picked yet. */
export function emptyMatchRow(): MatchRow {
	return { reference: '', quantifier: null, test: { kind: 'equals', value: '' } };
}

/** A require row with nothing picked yet. */
export function emptyRequireRow(): RequireRow {
	return { reference: '', test: { kind: 'required' } };
}

/** The require test of `kind` with nothing entered. */
export function emptyRequireTest(kind: RequireTest['kind']): RequireTest {
	switch (kind) {
		case 'required':
		case 'forbidden':
			return { kind };
		case 'values':
		case 'not':
			return { kind, values: [] };
		case 'compare':
			return { kind, comparison: 'eq', field: '' };
		case 'min_count':
		case 'max_count':
			return { kind, count: 1 };
	}
}

/**
 * The match row after its reference changed. The test is kept when the
 * new reference still takes it and reset otherwise; a quantifier is
 * added or dropped as the reference needs one (`any` by default, the
 * usual question: "does any child…").
 */
export function withMatchReference(
	row: MatchRow,
	reference: Reference,
	anchors: Anchor[]
): MatchRow {
	const kind = referenceKind(reference, anchors);
	const allowed = matchTestChoices(kind);
	const test = allowed.includes(choiceOfTest(row.test))
		? row.test
		: testOfChoice(allowed[0] ?? 'equals');
	return {
		reference: joinReference(reference) ?? '',
		quantifier: needsQuantifier(kind) ? (row.quantifier ?? 'any') : null,
		test
	};
}

/** The require row after its reference changed; the test is kept when still allowed, reset otherwise. */
export function withRequireReference(
	row: RequireRow,
	reference: Reference,
	anchors: Anchor[],
	targetType: FieldType | null,
	tables: RuleTables
): RequireRow {
	const allowed = requireTestKinds(referenceKind(reference, anchors), targetType, tables);
	const test = allowed.includes(row.test.kind)
		? row.test
		: emptyRequireTest(allowed[0] ?? 'required');
	return { reference: joinReference(reference) ?? '', test };
}

// ── What greys out Save ───────────────────────────────────────────────

/**
 * Why the draft is not ready to be judged, or `null`: a blank name, a
 * row that names no field yet, a test with nothing entered. These are
 * the gaps a form has while it is being filled in, which the server's
 * verdict would word as a refusal of nothing ("no field named ''").
 * Everything else — the name's spelling and uniqueness, a reference to
 * no field, the same test twice, a count without a bound, a rule with
 * nothing to require — is the server's call, made by the preview on
 * every pause and worded as the parser words it, so no rule is kept in
 * two places.
 */
export function ruleProblem(draft: RuleDraft): string | null {
	if (draft.name.trim() === '') return 'Enter a name.';
	for (const row of draft.shape.match_rows) {
		if (row.reference === '') return 'Every condition needs a field.';
		const value = scalarTestProblem(row.test);
		if (value !== null) return value;
	}
	for (const row of draft.shape.require_rows) {
		if (row.reference === '') return 'Every requirement needs a field.';
		const value = requireTestProblem(row.test);
		if (value !== null) return value;
	}
	return null;
}

function scalarTestProblem(test: ScalarTest): string | null {
	switch (test.kind) {
		case 'equals':
			return test.value === '' ? 'Every condition needs a value.' : null;
		case 'one_of':
		case 'not':
			return test.values.length === 0 ? 'Every condition needs at least one value.' : null;
		case 'is_set':
			return null;
	}
}

function requireTestProblem(test: RequireTest): string | null {
	switch (test.kind) {
		case 'required':
		case 'forbidden':
		case 'min_count':
		case 'max_count':
			return null;
		case 'values':
		case 'not':
			return test.values.length === 0 ? 'Every requirement needs at least one value.' : null;
		case 'compare':
			return test.field === '' ? 'Pick a field to compare with.' : null;
	}
}
