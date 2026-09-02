// The announcement's pure logic — what [[timer-notifications]] decides
// without touching the DOM: which phases have a zero at all, when a
// countdown has crossed it, and what the tab title says. The noisy
// half (audio, Notification, the worker) lives in the announcer
// adapter; everything here is testable under the pure-module setup.

import type { TimerPhase } from '$lib/api/generated/TimerPhase';
import { formatCountdown } from './timerMath';

/**
 * The identity of a counting-down phase: its kind plus the moment it
 * started, unique for as long as the phase exists and never reused.
 * `null` when nothing counts toward zero — idle, or a stopwatch
 * session, which has no zero to announce. The key is what "announce
 * each crossing exactly once" is scoped to, in this tab (the detector
 * below) and across tabs (the announcer's claim).
 */
export function countdownKey(phase: TimerPhase): string | null {
	if (phase.phase === 'work') {
		return phase.phase_length_seconds === null ? null : `work:${String(phase.started_at_ms)}`;
	}
	if (phase.phase === 'break') {
		return `break:${String(phase.started_at_ms)}`;
	}
	return null;
}

/** One look at a running countdown: which phase (a `countdownKey`),
 * and how many seconds it still has — zero or negative in overrun. */
export interface CountdownObservation {
	key: string;
	kind: 'work' | 'break';
	remainingSeconds: number;
}

/**
 * The live-crossing rule (item decision 9): a crossing counts only
 * when this tab saw the same phase on the positive side first, so a
 * tab that loads into an already-overrun phase shows the state but
 * announces nothing. Zero itself is "over" — a countdown at `0:00`
 * has ended. Each key fires at most once, whatever the observations
 * do afterwards (a refetch may nudge remaining back above zero).
 *
 * A factory holding its own previous observation, so the store owns
 * exactly one detector and tests own as many as they like.
 */
export function createCrossingDetector(): (
	observation: CountdownObservation | null
) => CountdownObservation | null {
	let previous: CountdownObservation | null = null;
	const announced = new Set<string>();
	return (observation) => {
		const before = previous;
		previous = observation;
		if (observation === null) return null;
		const crossed =
			before !== null &&
			before.key === observation.key &&
			before.remainingSeconds > 0 &&
			observation.remainingSeconds <= 0 &&
			!announced.has(observation.key);
		if (!crossed) return null;
		announced.add(observation.key);
		return observation;
	};
}

/**
 * The tab title (item decision 7): the countdown whenever a pomodoro
 * phase runs — glanceable from a background tab before zero, not only
 * at it — flipping to an alarm form once the phase is over. The break
 * carries its word, as in the pill: a bare `4:12` would read as work.
 *
 * `base` is the title the page would have without a timer — the project
 * and page from `$lib/ui/documentTitle`. The countdown decorates it and
 * never replaces it: whichever project's tab is counting down stays
 * identifiable.
 */
export function tabTitle(
	countdown: { kind: 'work' | 'break'; remainingSeconds: number } | null,
	base: string
): string {
	if (countdown === null) return base;
	if (countdown.remainingSeconds <= 0) {
		return countdown.kind === 'work' ? `⏰ Interval over · ${base}` : `⏰ Break over · ${base}`;
	}
	const clock = formatCountdown(countdown.remainingSeconds);
	return countdown.kind === 'work' ? `${clock} · ${base}` : `Break ${clock} · ${base}`;
}
