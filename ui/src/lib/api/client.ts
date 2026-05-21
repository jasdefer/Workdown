// Typed HTTP client wrapping the API envelope.
//
// The envelope is `{ data?: T, diagnostics: Diagnostic[] }` and is the
// same shape for every endpoint. Centralising the unwrap here keeps
// every call site free of optional-chaining boilerplate on
// `diagnostics`.
//
// No endpoint-specific methods exist yet — slice 2 adds them as each
// endpoint lands. The `request<T>` helper is the only export today.
//
// `Diagnostic` will be imported from the generated types once an
// endpoint actually exchanges diagnostics; for now the helper types it
// as `unknown[]` so generated/ can stay empty of wire-level types
// until they're earned.

export interface ApiResult<T> {
	data?: T;
	diagnostics: unknown[];
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

	// 204 (and any empty body) is normalised to `{ diagnostics: [] }`
	// so callers never see a parse error from `.json()` on an empty body.
	const text = await response.text();
	const envelope =
		text.length > 0 ? (JSON.parse(text) as { data?: T; diagnostics?: unknown[] }) : {};

	// Same conditional-spread pattern for `data` — omitted on absence,
	// not set to `undefined`.
	return {
		...(envelope.data !== undefined ? { data: envelope.data } : {}),
		diagnostics: envelope.diagnostics ?? [],
		status: response.status
	};
}

// Endpoint methods land here as slice 2 wires the first one:
//
//   export const api = {
//     getView: (id: string) => request<ViewData>('GET', `/api/views/${id}`),
//     ...
//   };
