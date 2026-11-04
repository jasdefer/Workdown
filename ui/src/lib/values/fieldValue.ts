// Reading a field value without knowing its shape. A value on the wire
// is a scalar or a list depending on the field's type; the callers here
// want "the strings it names" either way — the ids of a `link` or
// `links` field, the entries of a `string` or `list` field backed by a
// resource — so one coercion serves both rather than each editor
// re-deciding what an absent or scalar value means as a list.

import type { FieldValue } from '$lib/api/generated/FieldValue';

/**
 * The strings a value names, as a list either way: a list's string
 * entries, a non-empty string on its own, nothing for an absent value,
 * an empty string, or a value of another type.
 */
export function stringsOf(value: FieldValue | null): string[] {
	if (Array.isArray(value)) return value.filter((entry) => typeof entry === 'string');
	return typeof value === 'string' && value.length > 0 ? [value] : [];
}
