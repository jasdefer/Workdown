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

import type { Clause } from './generated/Clause';
import type { CreateItem } from './generated/CreateItem';
import type { CreateItemResult } from './generated/CreateItemResult';
import type { CreateView } from './generated/CreateView';
import type { Diagnostic } from './generated/Diagnostic';
import type { FieldMutation } from './generated/FieldMutation';
import type { FieldMutationResult } from './generated/FieldMutationResult';
import type { ItemDetail } from './generated/ItemDetail';
import type { SchemaData } from './generated/SchemaData';
import type { SetViewFilter } from './generated/SetViewFilter';
import type { UpdateView } from './generated/UpdateView';
import type { ViewData } from './generated/ViewData';
import type { ViewDefinition } from './generated/ViewDefinition';
import type { ViewMutationResult } from './generated/ViewMutationResult';
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
	/**
	 * Fetch a view's data. `filter` is a JSON array of structured clauses
	 * (already serialized) for a "for right now" preview: the server
	 * extracts with those clauses instead of the persisted `where:`, and
	 * writes nothing. `display` is a JSON object of display roles for a
	 * per-session override — set roles take highest precedence; nothing
	 * is persisted.
	 */
	getView: (id: string, filter?: string, display?: string) => {
		const params = new URLSearchParams();
		if (filter !== undefined) params.set('filter', filter);
		if (display !== undefined) params.set('display', display);
		const query = params.toString();
		return request<ViewData>(
			'GET',
			`/api/views/${encodeURIComponent(id)}${query ? `?${query}` : ''}`
		);
	},
	/** The view's persisted filter, decomposed into the editor's clause shape. */
	getViewFilter: (id: string) =>
		request<Clause[]>('GET', `/api/views/${encodeURIComponent(id)}/filter`),
	/** Persist a view's filter (structured clauses) to `views.yaml`. */
	patchViewFilter: (id: string, clauses: Clause[]) =>
		request<ViewMutationResult>('PATCH', `/api/views/${encodeURIComponent(id)}`, {
			clauses
		} satisfies SetViewFilter),
	/**
	 * Create a view. `name` is slugged to the id server-side; `definition`
	 * is the kind + slots (no id); `filter` is the optional structured filter.
	 */
	createView: (body: CreateView) => request<ViewMutationResult>('POST', '/api/views', body),
	/**
	 * The persisted view decomposed for the edit form: the flat definition
	 * (no id, no where) plus the filter as structured clauses — exactly the
	 * shape `updateView` takes back.
	 */
	getViewDefinition: (id: string) =>
		request<ViewDefinition>('GET', `/api/views/${encodeURIComponent(id)}/definition`),
	/**
	 * Replace a view's whole definition. A non-null `name` renames the view
	 * (re-slugged server-side); the result's `view_id` is the id after the
	 * write, so navigate by it.
	 */
	updateView: (id: string, body: UpdateView) =>
		request<ViewMutationResult>('PUT', `/api/views/${encodeURIComponent(id)}`, body),
	/** Delete a view from `views.yaml` (and its stale rendered file). */
	deleteView: (id: string) =>
		request<ViewMutationResult>('DELETE', `/api/views/${encodeURIComponent(id)}`),
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
