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

import type { FieldDefinitionData } from '$lib/api/generated/FieldDefinitionData';
import type { FieldDefinitionWrite } from '$lib/api/generated/FieldDefinitionWrite';
import type { FieldProperty } from '$lib/api/generated/FieldProperty';
import type { FieldShape } from '$lib/api/generated/FieldShape';
import type { FieldType } from '$lib/api/generated/FieldType';
import type { FieldValue } from '$lib/api/generated/FieldValue';
import type { Generator } from '$lib/api/generated/Generator';
import type { SchemaDefinitionData } from '$lib/api/generated/SchemaDefinitionData';
import type { ShapeKind } from '$lib/api/generated/ShapeKind';
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
}

/** The tables the draft functions read, as the definition payload serves them. */
export type TypeTables = Pick<SchemaDefinitionData, 'properties_by_type' | 'generators_by_type'>;

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
		shape: field.shape
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
		shape: emptyShape(shapeKindOf(tables, fieldType))
	};
}

/**
 * The draft after the type selector changed. The header block is kept.
 * The shape is kept when the new type is edited as the same shape
 * (`integer` to `float` keeps the bounds) and replaced by the empty one
 * otherwise. The default is dropped: a literal of the old type need not
 * fit the new one, and a generator need not be allowed for it. The
 * resource is kept only where the new type takes one.
 */
export function retype(tables: TypeTables, draft: FieldDraft, fieldType: FieldType): FieldDraft {
	const kind = shapeKindOf(tables, fieldType);
	return {
		...draft,
		fieldType,
		default: null,
		resource: propertiesOf(tables, fieldType).includes('resource') ? draft.resource : null,
		shape: draft.shape.kind === kind ? draft.shape : emptyShape(kind)
	};
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

/** The recipe keys: set by a fill mechanism, never by this editor. */
const RECIPE_PROPERTIES: FieldProperty[] = ['compute', 'aggregate', 'pull'];

/** The properties the panel has a control for: the bounds, text and values blocks, the resource control. */
const PROPERTIES_WITH_EDITOR: FieldProperty[] = ['min', 'max', 'pattern', 'resource', 'values'];

/**
 * The properties of `fieldType` this editor has no control for yet,
 * so the panel can say they are edited in the file for now. The
 * type-specific blocks each add their properties to
 * `PROPERTIES_WITH_EDITOR` as they land; the recipe keys never appear,
 * they are not plain properties.
 */
export function settingsWithoutEditor(tables: TypeTables, fieldType: FieldType): FieldProperty[] {
	return propertiesOf(tables, fieldType).filter(
		(property) =>
			!RECIPE_PROPERTIES.includes(property) && !PROPERTIES_WITH_EDITOR.includes(property)
	);
}

/** Whether a field of `fieldType` may name a resource list. */
export function takesResource(tables: TypeTables, fieldType: FieldType): boolean {
	return propertiesOf(tables, fieldType).includes('resource');
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
	const target = direction === 'up' ? index - 1 : index + 1;
	if (index < 0 || index >= shape.values.length || target < 0 || target >= shape.values.length) {
		return shape;
	}
	const values = [...shape.values];
	const moved = values.splice(index, 1);
	values.splice(target, 0, ...moved);
	return { ...shape, values };
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
		shape: draft.shape
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
