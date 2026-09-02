import { describe, it, expect } from 'vitest';
import type { TimerPhase } from '$lib/api/generated/TimerPhase';
import {
	countdownKey,
	createCrossingDetector,
	tabTitle,
	type CountdownObservation
} from './announcements';

const idle: TimerPhase = { phase: 'idle' };

const pomodoroWork: TimerPhase = {
	phase: 'work',
	item_id: 'write-the-report',
	started_at_ms: 1_724_236_000_000,
	elapsed_seconds: 60,
	effort_before_seconds: null,
	mode: 'pomodoro',
	phase_length_seconds: 1500
};

const stopwatchWork: TimerPhase = {
	...pomodoroWork,
	mode: 'stopwatch',
	phase_length_seconds: null
};

const breakPhase: TimerPhase = {
	phase: 'break',
	followed_item: 'write-the-report',
	started_at_ms: 1_724_237_600_000,
	elapsed_seconds: 10,
	phase_length_seconds: 300
};

describe('countdownKey', () => {
	it('gives idle no key — nothing counts toward zero', () => {
		expect(countdownKey(idle)).toBeNull();
	});

	it('gives a stopwatch session no key — it has no zero', () => {
		expect(countdownKey(stopwatchWork)).toBeNull();
	});

	it('keys a pomodoro work interval by kind and start moment', () => {
		expect(countdownKey(pomodoroWork)).toBe('work:1724236000000');
	});

	it('keys a break by kind and start moment', () => {
		expect(countdownKey(breakPhase)).toBe('break:1724237600000');
	});

	it('tells two phases of the same kind apart by their start', () => {
		const later: TimerPhase = { ...pomodoroWork, started_at_ms: 1_724_240_000_000 };
		expect(countdownKey(later)).not.toBe(countdownKey(pomodoroWork));
	});
});

function work(remainingSeconds: number): CountdownObservation {
	return { key: 'work:1724236000000', kind: 'work', remainingSeconds };
}

describe('createCrossingDetector', () => {
	it('fires when a watched countdown reaches zero', () => {
		const observe = createCrossingDetector();
		expect(observe(work(2))).toBeNull();
		expect(observe(work(1))).toBeNull();
		expect(observe(work(0))).toEqual(work(0));
	});

	it('fires when a throttled tick skips straight past zero', () => {
		const observe = createCrossingDetector();
		expect(observe(work(30))).toBeNull();
		expect(observe(work(-45))).toEqual(work(-45));
	});

	it('fires at most once per phase identity', () => {
		const observe = createCrossingDetector();
		observe(work(1));
		expect(observe(work(0))).not.toBeNull();
		expect(observe(work(-1))).toBeNull();
		// A refetch nudging remaining back above zero re-arms nothing.
		observe(work(1));
		expect(observe(work(-1))).toBeNull();
	});

	it('stays quiet when the first sight of a phase is already overrun', () => {
		// The tab reopened Monday morning: state shown, no stale chime.
		const observe = createCrossingDetector();
		expect(observe(work(-7200))).toBeNull();
		expect(observe(work(-7201))).toBeNull();
	});

	it('stays quiet across a phase change', () => {
		// Work was stopped mid-countdown; the break appears near its own
		// zero. Positive-side sighting of one phase never vouches for
		// the next.
		const observe = createCrossingDetector();
		observe(work(300));
		const breakObservation: CountdownObservation = {
			key: 'break:1724237600000',
			kind: 'break',
			remainingSeconds: 0
		};
		expect(observe(breakObservation)).toBeNull();
	});

	it('stays quiet across an idle gap', () => {
		const observe = createCrossingDetector();
		observe(work(1));
		observe(null);
		expect(observe(work(-1))).toBeNull();
	});

	it('announces each phase of a loop on its own', () => {
		const observe = createCrossingDetector();
		observe(work(1));
		expect(observe(work(0))).not.toBeNull();
		const breakEnd: CountdownObservation = {
			key: 'break:1724237600000',
			kind: 'break',
			remainingSeconds: 0
		};
		observe({ ...breakEnd, remainingSeconds: 300 });
		expect(observe(breakEnd)).toEqual(breakEnd);
	});
});

describe('tabTitle', () => {
	// The page's own title, as `documentTitle` builds it — the countdown
	// decorates this, never replaces it.
	const base = 'Acme Backlog — Status Board';

	it('is the page title untouched when nothing counts down', () => {
		expect(tabTitle(null, base)).toBe(base);
	});

	it('carries the work countdown ahead of the page title', () => {
		expect(tabTitle({ kind: 'work', remainingSeconds: 1122 }, base)).toBe(`18:42 · ${base}`);
	});

	it('names the break beside its countdown', () => {
		expect(tabTitle({ kind: 'break', remainingSeconds: 252 }, base)).toBe(`Break 4:12 · ${base}`);
	});

	it('flips to the alarm form at zero', () => {
		expect(tabTitle({ kind: 'work', remainingSeconds: 0 }, base)).toBe(
			`⏰ Interval over · ${base}`
		);
		expect(tabTitle({ kind: 'break', remainingSeconds: 0 }, base)).toBe(`⏰ Break over · ${base}`);
	});

	it('keeps the alarm form through overrun', () => {
		expect(tabTitle({ kind: 'work', remainingSeconds: -452 }, base)).toBe(
			`⏰ Interval over · ${base}`
		);
	});
});
