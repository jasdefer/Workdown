// The view form's side of the `sort:` slot.
//
// On the wire (and in `views.yaml`) the slot is one sort key as a string
// or several as a list, each in the `workdown query --sort` grammar:
// `field`, `field:asc` or `field:desc`. The editor works in rows of
// field plus direction; these pure functions convert between the two.
// The grammar is parsed for real by the Rust core (`query::parse::
// parse_sort`), which also reports a key the schema cannot resolve —
// here the string is only split and joined, so a hand-written key the
// form does not understand is carried through untouched rather than
// dropped.

export type SortDirection = 'asc' | 'desc';

/** One editable sort key. */
export interface SortKey {
	/** Stable key for `{#each}`; never crosses the wire. */
	localId: number;
	/** Empty until a field is picked; such a row contributes no key. */
	field: string;
	direction: SortDirection;
}

/** Split one wire key into its field and direction. A bare field is ascending. */
export function parseSortKey(key: string, localId: number): SortKey {
	const colon = key.indexOf(':');
	if (colon < 0) {
		return { localId, field: key.trim(), direction: 'asc' };
	}
	const field = key.slice(0, colon).trim();
	const direction = key.slice(colon + 1).trim();
	return { localId, field, direction: direction === 'desc' ? 'desc' : 'asc' };
}

/**
 * The rows for a persisted `sort` value — a string, a list of strings, or
 * absent. Anything else (a stray number, say) seeds no rows; the server
 * would have refused to load such a file anyway.
 */
export function sortSlotToKeys(value: unknown, nextId: () => number): SortKey[] {
	if (typeof value === 'string') {
		return [parseSortKey(value, nextId())];
	}
	if (Array.isArray(value)) {
		return value
			.filter((entry): entry is string => typeof entry === 'string')
			.map((entry) => parseSortKey(entry, nextId()));
	}
	return [];
}

/** One row as a wire key: `field` for ascending, `field:desc` otherwise. */
export function serializeSortKey(key: SortKey): string {
	return key.direction === 'desc' ? `${key.field}:desc` : key.field;
}

/**
 * The wire value for the rows: a single string for one key, a list for
 * several, `undefined` for none — the short form the Rust side writes
 * back, so a round trip through the form leaves `views.yaml` as it was.
 * Rows without a field are skipped.
 */
export function sortKeysToSlot(keys: SortKey[]): string | string[] | undefined {
	const filled = keys.filter((key) => key.field !== '').map(serializeSortKey);
	if (filled.length === 0) return undefined;
	if (filled.length === 1) return filled[0];
	return filled;
}
