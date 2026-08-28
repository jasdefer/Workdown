import { describe, it, expect, vi, afterEach } from 'vitest';

import { request } from './client';

/** Stub `fetch` with one canned `Response`, and hand back the mock so a
 * test can assert what the client asked for. `null` is an empty body —
 * the `Response` constructor rejects `''` on a 204. */
function respondWith(body: string | null, status = 200): ReturnType<typeof vi.fn> {
	const fetchMock = vi.fn(() => Promise.resolve(new Response(body, { status })));
	vi.stubGlobal('fetch', fetchMock);
	return fetchMock;
}

afterEach(() => {
	vi.unstubAllGlobals();
});

// Every call site reads `data`, `diagnostics` and `error` without
// optional-chaining, and several are fire-and-forget. Both of those
// hold only because this function never throws and never returns a
// `diagnostics` that is absent.
describe('request', () => {
	it('unwraps a full envelope', async () => {
		respondWith(JSON.stringify({ data: { id: 'task-1' }, diagnostics: [], error: 'nope' }));

		const result = await request<{ id: string }>('GET', '/api/items/task-1');

		expect(result).toEqual({
			data: { id: 'task-1' },
			diagnostics: [],
			error: 'nope',
			status: 200
		});
	});

	it('supplies the empty diagnostics list the callers assume', async () => {
		respondWith(JSON.stringify({ data: 1 }));

		expect((await request('GET', '/api/thing')).diagnostics).toEqual([]);
	});

	// 204, and any other empty body (a bare 404, say). `.json()` would
	// throw on these; the caller gets the empty envelope instead.
	it('normalizes an empty body to the empty envelope', async () => {
		respondWith(null, 204);

		const result = await request('POST', '/api/timer/stop', {});

		expect(result).toEqual({ diagnostics: [], status: 204 });
	});

	// Absent, not present-and-undefined: tsconfig runs with
	// `exactOptionalPropertyTypes`, and the conditional spread is what
	// keeps the two apart.
	it('omits data and error rather than setting them undefined', async () => {
		respondWith(null, 404);

		const result = await request('GET', '/api/items/nope');

		expect('data' in result).toBe(false);
		expect('error' in result).toBe(false);
	});

	it('turns a truncated reply into an error result rather than throwing', async () => {
		respondWith('{"data": {"id": "tas', 200);

		const result = await request('GET', '/api/items/task-1');

		expect(result.data).toBeUndefined();
		expect(result.error).toBeDefined();
		expect(result.diagnostics).toEqual([]);
	});

	// A rejection here would escape the fire-and-forget call sites and
	// strand their in-flight state — the timer's `busy` flag never
	// clearing, and its controls staying disabled for the tab's life.
	it('turns an unreachable server into status 0, not a rejection', async () => {
		vi.stubGlobal(
			'fetch',
			vi.fn(() => Promise.reject(new Error('Failed to fetch')))
		);

		const result = await request('GET', '/api/views');

		expect(result).toEqual({
			diagnostics: [],
			error: 'Failed to fetch',
			status: 0
		});
	});

	it('stands in a message when the rejection was not an Error', async () => {
		vi.stubGlobal(
			'fetch',
			// The branch under test is the one a non-Error rejection takes,
			// which is the thing this rule exists to prevent everywhere else.
			// eslint-disable-next-line @typescript-eslint/prefer-promise-reject-errors
			vi.fn(() => Promise.reject('nope'))
		);

		expect((await request('GET', '/api/views')).error).toBe(
			'The request failed to reach the server.'
		);
	});

	it('sends a body as JSON, and sends no body when there is none', async () => {
		const withBody = respondWith('{}');
		await request('POST', '/api/items', { title: 'New' });
		expect(withBody).toHaveBeenCalledWith('/api/items', {
			method: 'POST',
			headers: { 'content-type': 'application/json' },
			body: '{"title":"New"}'
		});

		const withoutBody = respondWith('{}');
		await request('GET', '/api/items');
		expect(withoutBody).toHaveBeenCalledWith('/api/items', { method: 'GET' });
	});
});
