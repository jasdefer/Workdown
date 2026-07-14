// Schema store — the project's editing vocabulary, fetched once from
// `GET /api/schema` and reused across the detail panel and the create
// form.
//
// Lazily loaded: the first `load()` fetches; concurrent callers share
// the in-flight promise; later callers are a no-op once cached. The item
// index (`items`, used by link/links pickers) goes stale when items are
// created or renamed, so `reload()` forces a refetch — callers invoke it
// after a successful create.

import { api } from '$lib/api/client';
import type { FieldSchema } from '$lib/api/generated/FieldSchema';
import type { FieldType } from '$lib/api/generated/FieldType';
import type { Operator } from '$lib/api/generated/Operator';
import type { SchemaData } from '$lib/api/generated/SchemaData';

let data = $state<SchemaData | null>(null);
let loadError = $state<string | null>(null);
let inFlight: Promise<void> | null = null;

async function fetchSchema(): Promise<void> {
	const result = await api.getSchema();
	if (result.data !== undefined) {
		data = result.data;
		loadError = null;
	} else {
		loadError = result.error ?? 'Failed to load schema';
	}
}

export const schemaStore = {
	/** The full payload, or `null` before the first successful load. */
	get value(): SchemaData | null {
		return data;
	},
	/** Field definitions in schema-declaration order (empty until loaded). */
	get fields(): FieldSchema[] {
		return data?.fields ?? [];
	},
	/** All work-item ids, sorted — the option set for link/links pickers. */
	get items(): string[] {
		return data?.items ?? [];
	},
	/** Set when the last load failed; `null` otherwise. */
	get error(): string | null {
		return loadError;
	},
	/** Look up a single field's editing metadata by name. */
	field(name: string): FieldSchema | undefined {
		return data?.fields.find((field) => field.name === name);
	},
	/**
	 * Operators the filter builder may offer for a field type — the set the
	 * evaluator treats as meaningful. Empty until loaded, or for an unknown
	 * type.
	 */
	operatorsFor(fieldType: FieldType): Operator[] {
		return data?.operators_by_type.find((entry) => entry.field_type === fieldType)?.operators ?? [];
	},
	/** Fetch once and cache. Idempotent; safe to call from many components. */
	async load(): Promise<void> {
		if (data !== null) return;
		inFlight ??= fetchSchema().finally(() => {
			inFlight = null;
		});
		return inFlight;
	},
	/** Force a refetch — e.g. after a create changed the item index. */
	async reload(): Promise<void> {
		inFlight = fetchSchema().finally(() => {
			inFlight = null;
		});
		return inFlight;
	}
};
