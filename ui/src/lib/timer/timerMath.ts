// The timer's pure arithmetic — everything the components compute is
// here, DOM-free, so it is testable under the pure-module test setup.
//
// `roundedWriteSeconds` mirrors `rounded_write_seconds` in
// `crates/core/src/timer_data.rs` line for line, so the projected write
// shown while the timer runs and the write the server performs on stop
// can never disagree. Both sides are tested on the same boundaries
// (29s → nothing, 30s → 1min, 90s → 2min).

/** Elapsed seconds rounded into the seconds a stop would write: nearest
 * whole minute, thirty seconds rounds up. Zero means stop writes nothing. */
export function roundedWriteSeconds(elapsedSeconds: number): number {
	return Math.floor((elapsedSeconds + 30) / 60) * 60;
}

/** What the effort field would hold after a stop right now. The absent
 * field (`null`) starts from zero, exactly as the server's delta does. */
export function projectedNewSeconds(
	effortBeforeSeconds: number | null,
	elapsedSeconds: number
): number {
	return (effortBeforeSeconds ?? 0) + roundedWriteSeconds(elapsedSeconds);
}

/**
 * The ticking elapsed time, clock-style — `12:03`, `1:23:45` — because a
 * "1h 23min" label does not visibly tick. Hours carry past twenty-four
 * without wrapping: the forgotten weekend reads `65:12:03`. Everything
 * the write touches uses `formatDurationSeconds` instead.
 */
export function formatClock(totalSeconds: number): string {
	const total = Math.max(0, Math.floor(totalSeconds));
	const hours = Math.floor(total / 3600);
	const minutes = Math.floor((total % 3600) / 60);
	const seconds = total % 60;
	const ss = String(seconds).padStart(2, '0');
	if (hours === 0) {
		return `${String(minutes)}:${ss}`;
	}
	return `${String(hours)}:${String(minutes).padStart(2, '0')}:${ss}`;
}

/**
 * The pomodoro countdown, clock-style and signed: `18:42` on the way
 * down, `−7:32` in overrun — the typographic minus (U+2212), not a
 * hyphen. Reaching zero stops nothing, so the figure keeps going and
 * carries hours without wrapping exactly as `formatClock` does: a
 * forgotten work interval reads `−64:47:10`.
 */
export function formatCountdown(remainingSeconds: number): string {
	if (remainingSeconds < 0) {
		return `−${formatClock(-remainingSeconds)}`;
	}
	return formatClock(remainingSeconds);
}

/**
 * Elapsed seconds right now, from the last server answer: "the server
 * said `anchorElapsedSeconds`, `nowMs - anchorMs` ago". Anchoring to a
 * monotonic local timestamp (performance.now) means a wrong browser
 * wall clock cannot skew the display; the clamp keeps a stale-looking
 * anchor from ever counting backwards.
 */
export function anchoredElapsedSeconds(
	anchorElapsedSeconds: number,
	anchorMs: number,
	nowMs: number
): number {
	return anchorElapsedSeconds + Math.max(0, Math.floor((nowMs - anchorMs) / 1000));
}
