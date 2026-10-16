// The server's rendering of a draft — the YAML it would be written as
// and the verdict on it — as the field and the rule panel both keep it.
// A draft goes to its preview endpoint after a pause in editing; the
// answer is keyed by the exact request it was for, so an answer to an
// older draft is dropped on arrival and a panel's Save guard only ever
// trusts the answer to the draft as it stands.
//
// The key is the request's JSON: comparing text is how two drafts are
// told apart, and parsing it back is how the request leaves the
// reactive proxy the draft lives in before it is sent.

import type { ApiResult } from '$lib/api/client';

/** What every preview endpoint answers, whatever else it adds. */
export interface PreviewAnswer {
	yaml: string;
	problem: string | null;
}

export type PreviewState<Answer extends PreviewAnswer> =
	| { status: 'idle' }
	| { status: 'failed'; message: string }
	| { status: 'loaded'; key: string; answer: Answer };

export interface Preview<Body, Answer extends PreviewAnswer> {
	/** The last answer, or why there is none. */
	readonly state: PreviewState<Answer>;
	/**
	 * Ask for `body` once editing pauses. Returns the cancel for an
	 * effect's cleanup, so a draft replaced before the pause ends is
	 * never sent.
	 */
	schedule(body: Body): () => void;
	/** The answer to `body`; `null` until the server has answered that exact draft. */
	answerFor(body: Body): Answer | null;
}

/** How long editing has to pause before a draft is sent. */
const PAUSE_MS = 300;

/** A preview whose drafts `load` sends. */
export function createPreview<Body, Answer extends PreviewAnswer>(
	load: (body: Body) => Promise<ApiResult<Answer>>
): Preview<Body, Answer> {
	let state = $state<PreviewState<Answer>>({ status: 'idle' });
	let timer: ReturnType<typeof setTimeout> | undefined;
	let latestKey = '';

	async function fetchAnswer(key: string): Promise<void> {
		const result = await load(JSON.parse(key) as Body);
		if (key !== latestKey) return;
		if (result.data === undefined) {
			state = { status: 'failed', message: result.error ?? 'The preview could not be loaded.' };
			return;
		}
		state = { status: 'loaded', key, answer: result.data };
	}

	return {
		get state() {
			return state;
		},
		schedule(body) {
			const key = JSON.stringify(body);
			latestKey = key;
			clearTimeout(timer);
			timer = setTimeout(() => {
				void fetchAnswer(key);
			}, PAUSE_MS);
			return () => {
				clearTimeout(timer);
			};
		},
		answerFor(body) {
			const key = JSON.stringify(body);
			return state.status === 'loaded' && state.key === key ? state.answer : null;
		}
	};
}
