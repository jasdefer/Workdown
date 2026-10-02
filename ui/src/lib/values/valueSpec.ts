// What `ValueEditor` needs to know about a value: its type, the
// constraints that shape its input (allowed values, numeric bounds), and
// whether clearing it is allowed. A deliberately small type with no
// knowledge of where the value lives — an item's field or a schema
// field's default — so one editor serves both. Each host builds a spec
// from what it has: `specOfFieldSchema` here from the item editors'
// payload, `specOfDraft` in `$lib/schema/fieldDraft` from the schema
// editor's draft.

import type { FieldSchema } from '$lib/api/generated/FieldSchema';
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
