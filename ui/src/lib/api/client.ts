// Typed HTTP client wrapping the API envelope.
//
// The envelope is `{ data?: T, diagnostics: Diagnostic[] }` and is the
// same shape for every endpoint. Centralising the unwrap here keeps
// every call site free of optional-chaining boilerplate on
// `diagnostics`.

import type { Diagnostic } from './generated/Diagnostic';
import type { ViewData } from './generated/ViewData';
import type { ViewSummary } from './generated/ViewSummary';

export interface ApiResult<T> {
	data?: T;
	diagnostics: Diagnostic[];
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
		text.length > 0 ? (JSON.parse(text) as { data?: T; diagnostics?: Diagnostic[] }) : {};

	// Same conditional-spread pattern for `data` — omitted on absence,
	// not set to `undefined`.
	return {
		...(envelope.data !== undefined ? { data: envelope.data } : {}),
		diagnostics: envelope.diagnostics ?? [],
		status: response.status
	};
}

export const api = {
	getViews: () => request<ViewSummary[]>('GET', '/api/views'),
	getView: (id: string) => request<ViewData>('GET', `/api/views/${encodeURIComponent(id)}`)
};
