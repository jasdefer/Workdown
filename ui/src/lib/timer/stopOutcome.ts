// The two decisions a stop leads to, pulled out of the store so they can
// be tested without one — the same split `announcements.ts` makes, and
// for the same reason: the choice is pure, the plumbing around it is not.
//
// Both are paths a person using the app will not see go wrong. Undo
// writes into a work item and reports success either way, so a wrong
// payload is data damage with no symptom. The stale-tab stop only fires
// in a race that daily use does not reach. Everything else the store
// does — the toast appearing, the controls disabling, the countdown
// ticking — announces its own failure immediately, and stays there.

import type { FieldMutation } from '$lib/api/generated/FieldMutation';
import type { TimerWrite } from '$lib/api/generated/TimerWrite';

/**
 * The field write that takes back a stop.
 *
 * `previous_value` is the frontmatter value from before the write,
 * verbatim, and it is put back verbatim — the user's spelling of a
 * duration survives the round trip. An absent field is `null` and must
 * come back absent, which is `unset` rather than a `replace` with
 * nothing: replacing with an empty value would leave the field present
 * and wrong.
 *
 * Note what is *not* asked here: whether the previous value is truthy.
 * Zero, `false` and the empty string are values a field held, and undo
 * has to restore them as values.
 */
export function undoMutation(write: TimerWrite): FieldMutation {
	const previous = write.previous_value;
	if (previous === null || previous === undefined) {
		return { op: 'unset' };
	}
	return { op: 'replace', value: previous };
}

/** What the store needs to know about a stop that brought back no
 * result. */
export interface StopFailure {
	/** The server's one line, or a stand-in when the request never got
	 * an answer at all. */
	message: string;
	/**
	 * The `409` family: there was no work interval to stop, so it is
	 * this tab that is out of date rather than the write that failed.
	 *
	 * One fact, two uses — it decides the advice line (another stop
	 * would meet the same refusal, so the toast must not suggest one)
	 * and it is why this failure alone resyncs.
	 */
	nothingToStop: boolean;
}

/**
 * Read a failed stop. `status` is the API client's — `0` when the
 * request never reached the server, in which case the interval is still
 * running and stopping again is exactly the right advice.
 */
export function stopFailure(status: number, error: string | undefined): StopFailure {
	return {
		message: error ?? 'Stopping the timer failed.',
		nothingToStop: status === 409
	};
}
