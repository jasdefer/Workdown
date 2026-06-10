// Typed HTTP client wrapping the API envelope.
//
// The envelope is `{ data?: T, diagnostics: Diagnostic[], error?: string }`
// and is the same shape for every endpoint. Centralising the unwrap here
// keeps every call site free of optional-chaining boilerplate on
// `diagnostics`.
//
// `error` is present only on a hard operational failure (the request was
// understood but couldn't be carried out — unknown item, invalid op, I/O
// error). Save-with-warning successes return `data` + `diagnostics` with
// no `error`. See the server's `envelope.rs` for the full contract.

import type { CreateItem } from './generated/CreateItem';
import type { CreateItemResult } from './generated/CreateItemResult';
import type { Diagnostic } from './generated/Diagnostic';
import type { FieldMutation } from './generated/FieldMutation';
import type { FieldMutationResult } from './generated/FieldMutationResult';
import type { ItemDetail } from './generated/ItemDetail';
import type { SchemaData } from './generated/SchemaData';
import type { ViewData } from './generated/ViewData';
import type { ViewSummary } from './generated/ViewSummary';

export interface ApiResult<T> {
	data?: T;
	diagnostics: Diagnostic[];
	error?: string;
	status: number;
}

export async function request<T>(
	method: string,
	path: string,
	body?: unknown
): Promise<ApiResult<T>> {
	// Build the RequestInit conditionally rather than setting fields
	// to `undefined` — with tsconfig's `exactOptionalPropertyTypes`,
	// `body: undefined` is rejected (the spec types `body` as
	// `BodyInit | null`, no `undefined`).
	const init: RequestInit =
		body !== undefined
			? {
					method,
					headers: { 'content-type': 'application/json' },
					body: JSON.stringify(body)
				}
			: { method };

	const response = await fetch(path, init);

	// 204 (and any empty body — e.g. 404) is normalised to
	// `{ diagnostics: [] }` so callers never see a parse error from
	// `.json()` on an empty body.
	const text = await response.text();
	const envelope =
		text.length > 0
			? (JSON.parse(text) as { data?: T; diagnostics?: Diagnostic[]; error?: string })
			: {};

	// Same conditional-spread pattern for `data`/`error` — omitted on
	// absence, not set to `undefined` (exactOptionalPropertyTypes).
	return {
		...(envelope.data !== undefined ? { data: envelope.data } : {}),
		diagnostics: envelope.diagnostics ?? [],
		...(envelope.error !== undefined ? { error: envelope.error } : {}),
		status: response.status
	};
}

export const api = {
	getViews: () => request<ViewSummary[]>('GET', '/api/views'),
	getView: (id: string) => request<ViewData>('GET', `/api/views/${encodeURIComponent(id)}`),
	getSchema: () => request<SchemaData>('GET', '/api/schema'),
	getItem: (id: string) => request<ItemDetail>('GET', `/api/items/${encodeURIComponent(id)}`),
	setField: (id: string, field: string, mutation: FieldMutation) =>
		request<FieldMutationResult>(
			'POST',
			`/api/items/${encodeURIComponent(id)}/fields/${encodeURIComponent(field)}`,
			mutation
		),
	createItem: (body: CreateItem) => request<CreateItemResult>('POST', '/api/items', body)
};
