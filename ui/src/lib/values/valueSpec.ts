// What `ValueEditor` needs to know about a value: its type, the
// constraints that shape its input (allowed values, numeric bounds), and
// whether clearing it is allowed. A deliberately small type with no
// knowledge of where the value lives — an item's field, a schema
// field's default or bound, a rule's value — so one editor serves all
// of them. Two constructors cover every host: `specOfFieldSchema` from
// the item editors' payload (`GET /api/schema`), `specOf` from a type
// and a shape of the schema editor's payload.

import type { FieldSchema } from '$lib/api/generated/FieldSchema';
import type { FieldShape } from '$lib/api/generated/FieldShape';
import type { FieldType } from '$lib/api/generated/FieldType';

export interface ValueSpec {
	fieldType: FieldType;
	/**
	 * Whether the value must be present. Clearing a required value hands
	 * the empty value to the host rather than `null`, so the server can
	 * say the field is missing.
	 */
	required: boolean;
	/** The allowed values of a `choice` or `multichoice`; empty otherwise. */
	values: string[];
	/** Inclusive bounds of an `integer` or `float`, when set. */
	min: number | null;
	max: number | null;
}

/** The spec for editing an item's field, from `GET /api/schema`. */
export function specOfFieldSchema(field: FieldSchema): ValueSpec {
	return {
		fieldType: field.field_type,
		required: field.required,
		values: field.values ?? [],
		min: field.min,
		max: field.max
	};
}

/**
 * The spec for one bare value of `fieldType` shaped by `shape`, as the
 * schema editor's payload spells it: a choice offers its values, a
 * number its bounds; `null` means no constraints at all. Never
 * required: every host that builds one — a field's default, a bound,
 * a rule's value — treats a cleared editor as "none", not as an error.
 */
export function specOf(fieldType: FieldType, shape: FieldShape | null): ValueSpec {
	return {
		fieldType,
		required: false,
		values: shape?.kind === 'values' ? shape.values : [],
		min: shape?.kind === 'numeric' ? shape.min : null,
		max: shape?.kind === 'numeric' ? shape.max : null
	};
}
