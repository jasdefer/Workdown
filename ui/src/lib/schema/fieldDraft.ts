// The field editor's working copy and what it is made from and into:
// the draft a panel opens with (from a served definition, or empty for
// a new field), the draft after a type change, the write body a save
// sends, and the spec the default control hands the value editor. Pure
// so every input shape can be unit-tested without a DOM; the panel
// component holds the draft and calls these.
//
// Nothing here knows a type's rules. The shape a type is edited as, the
// generators it accepts and the properties it allows are read from the
// tables `GET /api/schema/definition` serves; the type system stays in
// Rust, and a new type reaches this editor by changing the payload, not
// this file.

import type { AggregateConfig } from '$lib/api/generated/AggregateConfig';
import type { AggregateFunction } from '$lib/api/generated/AggregateFunction';
import type { ConditionLiteral } from '$lib/api/generated/ConditionLiteral';
import type { ConstantData } from '$lib/api/generated/ConstantData';
import type { FieldDefinitionData } from '$lib/api/generated/FieldDefinitionData';
import type { FieldDefinitionWrite } from '$lib/api/generated/FieldDefinitionWrite';
import type { FieldProperty } from '$lib/api/generated/FieldProperty';
import type { FieldRecipe } from '$lib/api/generated/FieldRecipe';
import type { FieldShape } from '$lib/api/generated/FieldShape';
import type { FieldType } from '$lib/api/generated/FieldType';
import type { FieldValue } from '$lib/api/generated/FieldValue';
import type { Generator } from '$lib/api/generated/Generator';
import type { Operand } from '$lib/api/generated/Operand';
import type { PullConfig } from '$lib/api/generated/PullConfig';
import type { SchemaDefinitionData } from '$lib/api/generated/SchemaDefinitionData';
import type { ShapeKind } from '$lib/api/generated/ShapeKind';
import type { WhenBranchData } from '$lib/api/generated/WhenBranchData';
import type { ValueSpec } from '$lib/values/valueSpec';

/**
 * The default as the panel edits it. A literal whose value is `null`
 * is the "fixed value" choice with nothing entered yet — it renders as
 * the empty editor and writes as no default.
 */
export type DraftDefault =
	| { kind: 'generator'; generator: Generator }
	| { kind: 'literal'; value: FieldValue | null };

/** The panel's working copy of one field. */
export interface FieldDraft {
	/** Free for a new field, fixed for an existing one. */
	name: string;
	fieldType: FieldType;
	required: boolean;
	/** As typed; blank writes as no description. */
	description: string;
	default: DraftDefault | null;
	resource: string | null;
	shape: FieldShape;
	/** The rollup (`aggregate:`), or none. */
	aggregate: AggregateConfig | null;
	/**
	 * The recipe filling the value (`pull:` so far), or none for a
	 * field set by hand.
	 */
	recipe: FieldRecipe | null;
}

/** The tables the draft functions read, as the definition payload serves them. */
export type TypeTables = Pick<
	SchemaDefinitionData,
	| 'properties_by_type'
	| 'generators_by_type'
	| 'aggregate_functions_by_type'
	| 'pull_sources_by_type'
	| 'expression_readable_types'
	| 'conditional_types'
>;

/**
 * The draft a panel opens with for an existing field. An invalid
 * default — text the type cannot hold — becomes a literal of that text,
 * so it is shown rather than silently dropped; the server judges it
 * again on save.
 */
export function initialDraft(field: FieldDefinitionData): FieldDraft {
	return {
		name: field.name,
		fieldType: field.field_type,
		required: field.required,
		description: field.description ?? '',
		default: draftDefaultOf(field),
		resource: field.resource,
		shape: field.shape,
		aggregate: field.aggregate,
		recipe: field.recipe
	};
}

function draftDefaultOf(field: Pick<FieldDefinitionData, 'default'>): DraftDefault | null {
	const served = field.default;
	if (served === null) return null;
	switch (served.kind) {
		case 'generator':
			return { kind: 'generator', generator: served.generator };
		case 'literal':
			return { kind: 'literal', value: served.value };
		case 'invalid':
			return { kind: 'literal', value: served.text };
	}
}

/** The empty draft for a new field of `fieldType`. */
export function newDraft(tables: TypeTables, fieldType: FieldType): FieldDraft {
	return {
		name: '',
		fieldType,
		required: false,
		description: '',
		default: null,
		resource: null,
		shape: emptyShape(shapeKindOf(tables, fieldType)),
		aggregate: null,
		recipe: null
	};
}

/**
 * The draft after the type selector changed. The header block is kept.
 * The shape is kept when the new type is edited as the same shape
 * (`integer` to `float` keeps the bounds) and replaced by the empty one
 * otherwise. The default is dropped: a literal of the old type need not
 * fit the new one, and a generator need not be allowed for it. The
 * resource, the rollup and the recipe are kept only where the new type
 * takes one.
 */
export function retype(tables: TypeTables, draft: FieldDraft, fieldType: FieldType): FieldDraft {
	const kind = shapeKindOf(tables, fieldType);
	const properties = propertiesOf(tables, fieldType);
	return {
		...draft,
		fieldType,
		default: null,
		resource: properties.includes('resource') ? draft.resource : null,
		shape: draft.shape.kind === kind ? draft.shape : emptyShape(kind),
		aggregate: properties.includes('aggregate') ? draft.aggregate : null,
		recipe:
			draft.recipe !== null && takesRecipe(tables, fieldType, draft.recipe) ? draft.recipe : null
	};
}

/**
 * Whether a field of `fieldType` may carry `recipe`: a `when` where the
 * server lists the type as conditional (the parser polices `when` with
 * its own check, so it is not a property of the type), the others
 * where the type lists the property.
 */
export function takesRecipe(
	tables: TypeTables,
	fieldType: FieldType,
	recipe: FieldRecipe
): boolean {
	if (recipe.kind === 'when') return tables.conditional_types.includes(fieldType);
	return propertiesOf(tables, fieldType).includes(recipe.kind);
}

/** The shape with every setting unset. */
export function emptyShape(kind: ShapeKind): FieldShape {
	switch (kind) {
		case 'scalar':
			return { kind: 'scalar' };
		case 'numeric':
			return { kind: 'numeric', min: null, max: null };
		case 'duration':
			return { kind: 'duration', min: null, max: null };
		case 'text':
			return { kind: 'text', pattern: null };
		case 'values':
			return { kind: 'values', values: [] };
		case 'relation':
			return { kind: 'relation', allow_cycles: null, inverse: null };
	}
}

/**
 * The shape a field of `fieldType` is edited as. The table has one row
 * per type by contract, so a type without a row is a payload the app
 * was not built against, not a case to paper over.
 */
export function shapeKindOf(tables: TypeTables, fieldType: FieldType): ShapeKind {
	const row = tables.properties_by_type.find((entry) => entry.field_type === fieldType);
	if (row === undefined) {
		throw new Error(`The schema definition has no properties row for type '${fieldType}'.`);
	}
	return row.shape_kind;
}

/** The type-restricted properties `fieldType` accepts; empty for an unknown type. */
export function propertiesOf(tables: TypeTables, fieldType: FieldType): FieldProperty[] {
	return (
		tables.properties_by_type.find((entry) => entry.field_type === fieldType)?.properties ?? []
	);
}

/** The generators that may be the default of a `fieldType` field. */
export function generatorsFor(tables: TypeTables, fieldType: FieldType): Generator[] {
	return (
		tables.generators_by_type.find((entry) => entry.field_type === fieldType)?.generators ?? []
	);
}

/** Whether a field of `fieldType` may name a resource list. */
export function takesResource(tables: TypeTables, fieldType: FieldType): boolean {
	return propertiesOf(tables, fieldType).includes('resource');
}

/** Whether a field of `fieldType` may carry a rollup (`aggregate:`). */
export function takesAggregate(tables: TypeTables, fieldType: FieldType): boolean {
	return propertiesOf(tables, fieldType).includes('aggregate');
}

/** The functions a rollup of a `fieldType` field may reduce with, in the server's order. */
export function aggregateFunctionsFor(
	tables: TypeTables,
	fieldType: FieldType
): AggregateFunction[] {
	return (
		tables.aggregate_functions_by_type.find((entry) => entry.field_type === fieldType)?.functions ??
		[]
	);
}

/**
 * The names of the relations a rollup may climb: the `link` fields
 * that forbid cycles, in declaration order. A `links` field cannot be
 * climbed (a parent is one item), and a relation that allows cycles
 * has no acyclic hierarchy to roll up in; the parser refuses both, so
 * the picker offers neither.
 */
export function rollupRelations(fields: FieldDefinitionData[]): string[] {
	return fields
		.filter(
			(field) =>
				field.field_type === 'link' &&
				field.shape.kind === 'relation' &&
				field.shape.allow_cycles === false
		)
		.map((field) => field.name);
}

/**
 * The rollup the panel starts with when it is switched on: the type's
 * first function over the first relation, so the draft is whole from
 * the first keystroke and the pickers show a choice rather than a
 * blank. `null` when there is nothing to choose from — the type has no
 * functions, or no relation forbids cycles.
 */
export function newRollup(
	tables: TypeTables,
	fields: FieldDefinitionData[],
	fieldType: FieldType
): AggregateConfig | null {
	const [firstFunction] = aggregateFunctionsFor(tables, fieldType);
	const [firstRelation] = rollupRelations(fields);
	if (firstFunction === undefined || firstRelation === undefined) return null;
	return { function: firstFunction, error_on_missing: false, over: firstRelation };
}

/** Whether a field of `fieldType` may pull its value from linked items (`pull:`). */
export function takesPull(tables: TypeTables, fieldType: FieldType): boolean {
	return propertiesOf(tables, fieldType).includes('pull');
}

/**
 * The names of the relations a pull may follow: the `link` and `links`
 * fields that forbid cycles, in declaration order. A pull follows a
 * link forward, one hop, so a many-valued relation is fine; one that
 * allows cycles has no acyclic graph to evaluate in, and the load
 * disables the field, so the picker leaves it out.
 */
export function pullRelations(fields: FieldDefinitionData[]): string[] {
	return fields
		.filter(
			(field) =>
				(field.field_type === 'link' || field.field_type === 'links') &&
				field.shape.kind === 'relation' &&
				field.shape.allow_cycles === false
		)
		.map((field) => field.name);
}

/** A field a pull may read, with the functions that reduce it into the pulling field's type. */
export interface PullSourceField {
	name: string;
	functions: AggregateFunction[];
}

/**
 * The fields a pull into a `fieldType` field may read, in declaration
 * order: every field whose type the served table lists as a source,
 * each with the functions whose result the pulling field holds. The
 * rule — which function over which type yields which type, and that a
 * whole number fits a `float` field — is the server's; this only joins
 * its table with the schema's fields.
 */
export function pullSources(
	tables: TypeTables,
	fields: FieldDefinitionData[],
	fieldType: FieldType
): PullSourceField[] {
	const sources =
		tables.pull_sources_by_type.find((entry) => entry.field_type === fieldType)?.sources ?? [];
	return fields.flatMap((field) => {
		const source = sources.find((entry) => entry.field_type === field.field_type);
		return source === undefined ? [] : [{ name: field.name, functions: source.functions }];
	});
}

/**
 * The pull the panel starts with when it is chosen: the first relation,
 * the first readable field and that field's first function, so the
 * draft is whole at once. `null` when there is nothing to choose from —
 * no relation forbids cycles, or no field can be reduced into the
 * type.
 */
export function newPull(
	tables: TypeTables,
	fields: FieldDefinitionData[],
	fieldType: FieldType
): PullConfig | null {
	const [firstRelation] = pullRelations(fields);
	const [firstSource] = pullSources(tables, fields, fieldType);
	const firstFunction = firstSource?.functions[0];
	if (firstRelation === undefined || firstSource === undefined || firstFunction === undefined) {
		return null;
	}
	return {
		over: firstRelation,
		field: firstSource.name,
		function: firstFunction,
		error_on_missing: false
	};
}

/** The `compute:` recipe as the panel edits it: the formula as text and its options. */
export type ComputeRecipe = Extract<FieldRecipe, { kind: 'compute' }>;

/** Whether a field of `fieldType` may be computed by a formula (`compute:`). */
export function takesCompute(tables: TypeTables, fieldType: FieldType): boolean {
	return propertiesOf(tables, fieldType).includes('compute');
}

/** The formula the panel starts with when "Computed" is chosen: empty, no options. */
export function newCompute(): ComputeRecipe {
	return { kind: 'compute', expression: '', round: 'nearest', error_on_missing: false };
}

/** Something a formula may name, offered for insertion: the text and a word on what it is. */
export interface ExpressionReference {
	text: string;
	detail: string;
}

/**
 * What a formula on the field named `self` may read, in the order the
 * insert list shows them: the schema's fields whose type the server
 * lists as readable, the field itself left out (a formula reading its
 * own field is a cycle), then the project constants as
 * `$constants.<name>` with their values, then `$today`. The algebra
 * stays in Rust: the server's lists say what is readable, this only
 * joins them with the schema's fields.
 */
export function expressionReferences(
	tables: TypeTables,
	fields: FieldDefinitionData[],
	constants: ConstantData[],
	self: string | null
): ExpressionReference[] {
	const readable = fields
		.filter(
			(field) => field.name !== self && tables.expression_readable_types.includes(field.field_type)
		)
		.map((field) => ({ text: field.name, detail: field.field_type }));
	const named = constants.map((constant) => ({
		text: `$constants.${constant.name}`,
		detail: `= ${String(constant.value)}`
	}));
	return [
		...readable,
		...named,
		{ text: '$today', detail: 'the date the project is evaluated on' }
	];
}

/**
 * `expression` with `text` put in place of the selection from `start`
 * to `end`, a space added on either side where the neighbour is not
 * one already (and not a bracket), so a click lands a token, not a
 * run-on. `caret` is where typing continues: after the inserted text
 * and its trailing space, if any.
 */
export function insertAt(
	expression: string,
	start: number,
	end: number,
	text: string
): { expression: string; caret: number } {
	const from = Math.max(0, Math.min(start, expression.length));
	const to = Math.max(from, Math.min(end, expression.length));
	const before = expression.slice(0, from);
	const after = expression.slice(to);
	const needsSpaceBefore = before !== '' && !/[\s(]$/.test(before);
	const needsSpaceAfter = after !== '' && !/^[\s)]/.test(after);
	const inserted = (needsSpaceBefore ? ' ' : '') + text + (needsSpaceAfter ? ' ' : '');
	return { expression: before + inserted + after, caret: before.length + inserted.length };
}

/**
 * The pull reading `name` instead. The function is kept when the new
 * source offers it and replaced by the source's first one otherwise,
 * so a `sum` survives a switch between two numbers and a date source
 * does not keep a function it cannot be reduced with.
 */
export function withPullSource(
	value: PullConfig,
	sources: PullSourceField[],
	name: string
): PullConfig {
	const functions = sources.find((source) => source.name === name)?.functions ?? [];
	const kept = functions.includes(value.function) ? value.function : functions[0];
	return { ...value, field: name, function: kept ?? value.function };
}

/** The `when:` recipe as the panel edits it: the branches in order, first match wins. */
export type WhenRecipe = Extract<FieldRecipe, { kind: 'when' }>;

/** Whether a field of `fieldType` may be conditional (`when:`): the server lists the types. */
export function takesWhen(tables: TypeTables, fieldType: FieldType): boolean {
	return tables.conditional_types.includes(fieldType);
}

/** One entry of an operand picker: a key a select can hold, its label, and the operand it stands for. */
export interface OperandChoice {
	key: string;
	label: string;
	operand: Operand;
}

/**
 * What a condition's operand may be, in the order the picker shows
 * them: the readable fields (the field itself left out, a condition
 * reading its own field is a cycle), the project constants, `$today`,
 * and "a value" entered through the value editor. Which fields are
 * readable the server's list says.
 */
export function operandChoices(
	tables: TypeTables,
	fields: FieldDefinitionData[],
	constants: ConstantData[],
	self: string | null
): OperandChoice[] {
	const fieldChoices: OperandChoice[] = fields
		.filter(
			(field) => field.name !== self && tables.expression_readable_types.includes(field.field_type)
		)
		.map((field) => ({
			key: `field:${field.name}`,
			label: field.name,
			operand: { kind: 'field', name: field.name }
		}));
	const constantChoices: OperandChoice[] = constants.map((constant) => ({
		key: `constant:${constant.name}`,
		label: `$constants.${constant.name}`,
		operand: { kind: 'constant', name: constant.name }
	}));
	return [
		...fieldChoices,
		...constantChoices,
		{ key: 'today', label: '$today', operand: { kind: 'today' } },
		{ key: 'literal', label: 'a value', operand: { kind: 'literal', value: '' } }
	];
}

/** The picker key of `operand`, the inverse of `operandChoices`' keys. */
export function operandKey(operand: Operand): string {
	switch (operand.kind) {
		case 'field':
			return `field:${operand.name}`;
		case 'constant':
			return `constant:${operand.name}`;
		case 'today':
			return 'today';
		case 'literal':
			return 'literal';
	}
}

/**
 * What a literal compared against `other` is edited as: the other
 * field's own spec (a choice offers its values, a number its bounds), a
 * constant's type read off its value, a date against `$today`, free
 * text against another literal. The server type-checks the comparison;
 * this only picks the editor.
 */
export function literalSpecFor(
	other: Operand,
	fields: FieldDefinitionData[],
	constants: ConstantData[]
): ValueSpec {
	const text: ValueSpec = {
		fieldType: 'string',
		required: false,
		values: [],
		min: null,
		max: null
	};
	switch (other.kind) {
		case 'field': {
			const field = fields.find((entry) => entry.name === other.name);
			return field === undefined
				? text
				: {
						fieldType: field.field_type,
						required: false,
						values: field.shape.kind === 'values' ? field.shape.values : [],
						min: field.shape.kind === 'numeric' ? field.shape.min : null,
						max: field.shape.kind === 'numeric' ? field.shape.max : null
					};
		}
		case 'constant': {
			const value = constants.find((entry) => entry.name === other.name)?.value;
			if (typeof value === 'number') {
				return { ...text, fieldType: Number.isInteger(value) ? 'integer' : 'float' };
			}
			if (typeof value === 'boolean') return { ...text, fieldType: 'boolean' };
			return text;
		}
		case 'today':
			return { ...text, fieldType: 'date' };
		case 'literal':
			return text;
	}
}

/**
 * The literal the value editor's output stands for: a number, a
 * boolean or text as they are, a cleared value as empty text. The
 * grammar has no date or collection literal, so those read as text
 * and the server's type check says so.
 */
export function literalOf(value: FieldValue | null): ConditionLiteral {
	if (typeof value === 'number' || typeof value === 'boolean' || typeof value === 'string') {
		return value;
	}
	if (typeof value === 'bigint') return Number(value);
	return '';
}

/** The literal as the value editor shows it. */
export function valueOfLiteral(literal: ConditionLiteral): FieldValue {
	return typeof literal === 'bigint' ? Number(literal) : literal;
}

/** The branch's `then`, which the wire types as unknown, as the value editor's input. */
export function thenOf(branch: WhenBranchData): FieldValue | null {
	return branch.then as FieldValue | null;
}

/**
 * A fresh branch: the first operand choice compared for equality
 * against a value not yet entered, and no `then` yet.
 */
export function newBranch(choices: OperandChoice[]): WhenBranchData {
	const left: Operand = choices[0]?.operand ?? { kind: 'today' };
	return {
		condition: {
			kind: 'row',
			left,
			operator: 'equal',
			right: { kind: 'literal', value: '' }
		},
		then: null
	};
}

/** The recipe "Conditional" starts with: one fresh branch. */
export function newWhen(choices: OperandChoice[]): WhenRecipe {
	return { kind: 'when', branches: [newBranch(choices)] };
}

/** The recipe with the branch at `index` replaced. */
export function withBranch(recipe: WhenRecipe, index: number, branch: WhenBranchData): WhenRecipe {
	return {
		...recipe,
		branches: recipe.branches.map((entry, at) => (at === index ? branch : entry))
	};
}

/** The recipe with `branch` appended. */
export function withBranchAdded(recipe: WhenRecipe, branch: WhenBranchData): WhenRecipe {
	return { ...recipe, branches: [...recipe.branches, branch] };
}

/** The recipe without the branch at `index`. */
export function withBranchRemoved(recipe: WhenRecipe, index: number): WhenRecipe {
	return { ...recipe, branches: recipe.branches.filter((_, at) => at !== index) };
}

/** The recipe with the branch at `index` moved one up or down; order is meaning, first match wins. */
export function withBranchMoved(
	recipe: WhenRecipe,
	index: number,
	direction: 'up' | 'down'
): WhenRecipe {
	const branches = moved(recipe.branches, index, direction);
	return branches === recipe.branches ? recipe : { ...recipe, branches };
}

/** A shape with inclusive bounds: numbers for `integer` and `float`, shorthand text for `duration`. */
export type BoundedShape = Extract<FieldShape, { kind: 'numeric' | 'duration' }>;

/** The shape of a `string`: an optional pattern the value must match. */
export type TextShape = Extract<FieldShape, { kind: 'text' }>;

/**
 * The shape with one bound replaced by what the value editor handed
 * over. The editor emits the type's own value — a number for a numeric
 * field, shorthand text for a duration — or `null` when cleared, which
 * unsets the bound and removes the key from the file. A value of the
 * wrong kind cannot come from the editor; it is treated as cleared
 * rather than written into a shape it does not fit.
 */
export function withBound(
	shape: BoundedShape,
	bound: 'min' | 'max',
	value: FieldValue | null
): BoundedShape {
	switch (shape.kind) {
		case 'numeric': {
			const next = typeof value === 'number' ? value : null;
			return bound === 'min' ? { ...shape, min: next } : { ...shape, max: next };
		}
		case 'duration': {
			const next = typeof value === 'string' && value !== '' ? value : null;
			return bound === 'min' ? { ...shape, min: next } : { ...shape, max: next };
		}
	}
}

/**
 * The text shape with its pattern replaced by what was typed; blank
 * means no pattern, which removes the key from the file. The text is
 * kept as typed otherwise — a regex may begin or end with a space.
 */
export function withPattern(shape: TextShape, pattern: string): TextShape {
	return { ...shape, pattern: pattern.trim() === '' ? null : pattern };
}

/** The shape of a `link` or `links`: the cycle policy and the name the relation has from the other side. */
export type RelationShape = Extract<FieldShape, { kind: 'relation' }>;

/**
 * Whether the block's "Forbid cycles" box is checked for `shape`. The
 * file has three spellings — `false`, `true` and nothing — but only
 * `false` turns the cycle check on, and only `false` lets a recipe
 * climb the relation; `true` and nothing behave the same. The block
 * therefore offers the one choice that matters.
 */
export function forbidsCycles(shape: RelationShape): boolean {
	return shape.allow_cycles === false;
}

/**
 * The shape with the cycle policy set from the box: checked writes
 * `allow_cycles: false`, unchecked removes the key. A field that said
 * `true` is written without the key on its next save, which means the
 * same thing (see `forbidsCycles`).
 */
export function withForbidCycles(shape: RelationShape, forbid: boolean): RelationShape {
	return { ...shape, allow_cycles: forbid ? false : null };
}

/**
 * The shape with its inverse replaced by what was typed, trimmed;
 * blank means no inverse, which removes the key from the file. Whether
 * the name is a valid identifier, or collides with a field or another
 * inverse, is the server's call — the same rules the field name itself
 * is judged by, kept in one place — and its message comes back to the
 * panel on save.
 */
export function withInverse(shape: RelationShape, text: string): RelationShape {
	const inverse = text.trim();
	return { ...shape, inverse: inverse === '' ? null : inverse };
}

/** The shape of a `choice` or `multichoice`: the allowed values, in the order boards and dropdowns show them. */
export type ValuesShape = Extract<FieldShape, { kind: 'values' }>;

/**
 * Why `text` cannot become a value of the list, or `null` when it can:
 * blank text names no option, and a value already listed would be the
 * same option twice. For a rename, `renaming` is the position whose
 * current value does not count as a clash with itself. The server
 * refuses both the same way; this only keeps the list clean as it is
 * edited, so a save is never refused for something the block let in.
 */
export function valueProblem(shape: ValuesShape, text: string, renaming?: number): string | null {
	const value = text.trim();
	if (value === '') return 'Enter a value.';
	const clash = shape.values.findIndex(
		(existing, index) => existing === value && index !== renaming
	);
	return clash === -1 ? null : `'${value}' is already in the list.`;
}

/** The shape with `text`, trimmed, appended as the last value. The caller has checked `valueProblem`. */
export function withValueAdded(shape: ValuesShape, text: string): ValuesShape {
	return { ...shape, values: [...shape.values, text.trim()] };
}

/**
 * The shape with the value at `index` replaced by `text`, trimmed. The
 * caller has checked `valueProblem`. Items holding the old value are
 * not touched; they warn after the save, as the panel says beside the
 * list.
 */
export function withValueRenamed(shape: ValuesShape, index: number, text: string): ValuesShape {
	return {
		...shape,
		values: shape.values.map((value, at) => (at === index ? text.trim() : value))
	};
}

/** The shape without the value at `index`. */
export function withValueRemoved(shape: ValuesShape, index: number): ValuesShape {
	return { ...shape, values: shape.values.filter((_, at) => at !== index) };
}

/**
 * The shape with the value at `index` swapped with its neighbour above
 * (`'up'`) or below (`'down'`). Unchanged at the list's ends, so a
 * button that is somehow clicked there does nothing.
 */
export function withValueMoved(
	shape: ValuesShape,
	index: number,
	direction: 'up' | 'down'
): ValuesShape {
	const values = moved(shape.values, index, direction);
	return values === shape.values ? shape : { ...shape, values };
}

/**
 * `list` with the entry at `index` swapped with its neighbour above
 * (`'up'`) or below (`'down'`). Unchanged at the list's ends, so a
 * button that is somehow clicked there does nothing.
 */
export function moved<T>(list: T[], index: number, direction: 'up' | 'down'): T[] {
	const target = direction === 'up' ? index - 1 : index + 1;
	if (index < 0 || index >= list.length || target < 0 || target >= list.length) {
		return list;
	}
	const next = [...list];
	const taken = next.splice(index, 1);
	next.splice(target, 0, ...taken);
	return next;
}

/**
 * Why the type-specific settings cannot be saved as they stand, or
 * `null`. Judged per shape, and only where the browser has the rule:
 * a numeric pair in the wrong order, a value list with nothing in it.
 * Duration bounds are text whose grammar lives in Rust, and a pattern
 * is a regex in Rust's dialect, which the browser's `RegExp` would
 * judge differently; both are the server's call, and the save comes
 * back with its message. Saves a round trip, nothing more: the server
 * refuses the same things.
 */
export function shapeProblem(shape: FieldShape): string | null {
	switch (shape.kind) {
		case 'numeric':
			return shape.min !== null && shape.max !== null && shape.min > shape.max
				? 'Min must not exceed max.'
				: null;
		case 'values':
			return shape.values.length === 0 ? 'Add at least one value.' : null;
		case 'scalar':
		case 'duration':
		case 'text':
		case 'relation':
			return null;
	}
}

/**
 * The body a save sends: the draft minus its name (the URL or the
 * create body carries it), with blank text as "none" and a fixed-value
 * choice with nothing entered as no default.
 */
export function writeBody(draft: FieldDraft): FieldDefinitionWrite {
	const description = draft.description.trim();
	const resource = draft.resource?.trim() ?? '';
	return {
		field_type: draft.fieldType,
		required: draft.required,
		description: description === '' ? null : description,
		default: writeDefaultOf(draft.default),
		resource: resource === '' ? null : resource,
		shape: draft.shape,
		aggregate: draft.aggregate,
		recipe: draft.recipe
	};
}

function writeDefaultOf(draft: DraftDefault | null): FieldDefinitionWrite['default'] {
	if (draft === null) return null;
	if (draft.kind === 'generator') return { kind: 'generator', generator: draft.generator };
	if (draft.value === null) return null;
	return { kind: 'literal', value: draft.value };
}

/**
 * What the value editor needs to edit the draft's fixed default. Never
 * required: clearing the editor means "no default", which `writeBody`
 * sends as `null`, not an empty value the server would have to refuse.
 */
export function specOfDraft(draft: Pick<FieldDraft, 'fieldType' | 'shape'>): ValueSpec {
	return {
		fieldType: draft.fieldType,
		required: false,
		values: draft.shape.kind === 'values' ? draft.shape.values : [],
		min: draft.shape.kind === 'numeric' ? draft.shape.min : null,
		max: draft.shape.kind === 'numeric' ? draft.shape.max : null
	};
}
