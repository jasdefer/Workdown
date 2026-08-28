import { describe, it, expect } from 'vitest';

import type { TimerWrite } from '$lib/api/generated/TimerWrite';
import { stopFailure, undoMutation } from './stopOutcome';

/** A stop's write record with only the field undo reads set meaningfully. */
function write(previousValue: unknown): TimerWrite {
	return {
		added_seconds: 1800,
		previous_value: previousValue,
		previous_seconds: null,
		new_seconds: 1800,
		mutation_caused_warning: false,
		info_messages: []
	};
}

// Undo is the one place in the app that writes to a work item and reports
// success whatever it wrote. There is no symptom to notice, so these are
// the assertions standing between a wrong branch and a corrupted field.
describe('undoMutation', () => {
	it('puts the previous value back verbatim', () => {
		expect(undoMutation(write('1h 30min'))).toEqual({ op: 'replace', value: '1h 30min' });
	});

	it('unsets the field when it held nothing before', () => {
		expect(undoMutation(write(null))).toEqual({ op: 'unset' });
	});

	it('unsets when the previous value is missing entirely', () => {
		expect(undoMutation(write(undefined))).toEqual({ op: 'unset' });
	});

	// The branch a truthiness test would get wrong: zero, `false` and the
	// empty string are values the field held, and undo restores them as
	// values rather than clearing the field.
	it('restores a falsy value as a value, not as an unset', () => {
		expect(undoMutation(write(0))).toEqual({ op: 'replace', value: 0 });
		expect(undoMutation(write(false))).toEqual({ op: 'replace', value: false });
		expect(undoMutation(write(''))).toEqual({ op: 'replace', value: '' });
	});

	it('preserves the shape of a structured previous value', () => {
		expect(undoMutation(write(['a', 'b']))).toEqual({ op: 'replace', value: ['a', 'b'] });
	});
});

// The 409 only happens when another tab stopped the interval first —
// a race daily use does not reach, which is exactly why it is here.
describe('stopFailure', () => {
	it('reads a 409 as this tab being stale, not as a failed write', () => {
		expect(stopFailure(409, 'No timer is running.')).toEqual({
			message: 'No timer is running.',
			nothingToStop: true
		});
	});

	it('reads a server error as a failed write, leaving the interval running', () => {
		expect(stopFailure(500, 'could not write the file')).toEqual({
			message: 'could not write the file',
			nothingToStop: false
		});
	});

	// Status 0 is the client's "no answer at all". Nothing was stopped
	// server-side, so stopping again is the right advice — the opposite
	// of the 409.
	it('reads an unanswered request as a failed write', () => {
		expect(stopFailure(0, 'Failed to fetch').nothingToStop).toBe(false);
	});

	it('stands in a message when the failure carried none', () => {
		expect(stopFailure(500, undefined).message).toBe('Stopping the timer failed.');
	});
});
