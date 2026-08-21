// Timer store — the one client-side copy of the server's timer state,
// shared by the item slot, the header pill and the toast.
//
// The server owns the timer; this store holds its last answer and ticks
// locally between refreshes, anchored as "the server said X seconds,
// Y moments ago" (see `timerMath.anchoredElapsedSeconds`), so a wrong
// browser clock cannot skew the display. It refetches on the timer-named
// live-update event (wired in the root layout) — deliberately not on the
// generic file-change ping.
//
// The toast lives here too: it is application-level (a stop can happen
// from any page), single-slot, and replaced by the next timer action —
// a queue would be dead machinery.

import { api } from '$lib/api/client';
import type { TimerState } from '$lib/api/generated/TimerState';
import type { TimerStopResult } from '$lib/api/generated/TimerStopResult';
import { anchoredElapsedSeconds } from '$lib/timer/timerMath';

/** What the single toast slot shows. `stopped` carries the whole stop
 * result (the write half is what undo needs); the failure kinds keep the
 * server's one-line error. */
export type TimerToast =
	| { kind: 'stopped'; result: TimerStopResult }
	| { kind: 'stop_failed'; message: string }
	| { kind: 'undone'; result: TimerStopResult }
	| { kind: 'undo_failed'; result: TimerStopResult; message: string };

export type StartResult = 'started' | 'needs_confirmation' | { error: string };

let data = $state<TimerState | null>(null);
// The local-monotonic moment `data` was received, and a once-a-second
// heartbeat that drives the ticking display while a timer runs.
let anchorMs = $state(0);
let nowMs = $state(0);
let panelOpen = $state(false);
let toast = $state<TimerToast | null>(null);
let busy = $state(false);
let loadPromise: Promise<void> | null = null;

function localNow(): number {
	return typeof performance !== 'undefined' ? performance.now() : 0;
}

function apply(state: TimerState): void {
	data = state;
	anchorMs = localNow();
	nowMs = anchorMs;
}

async function fetchState(): Promise<void> {
	const result = await api.getTimer();
	if (result.data !== undefined) {
		apply(result.data);
	}
}

// Tick once a second while a timer runs; idle otherwise. `$effect.root`
// because this module outlives any component (same pattern as the theme
// store).
$effect.root(() => {
	$effect(() => {
		const running = data?.running ?? null;
		if (running === null) return undefined;
		const interval = setInterval(() => {
			nowMs = localNow();
		}, 1000);
		return () => {
			clearInterval(interval);
		};
	});
});

export const timerStore = {
	get state(): TimerState | null {
		return data;
	},
	/** The id of the item being timed; `null` when idle. What the views
	 * compare against to mark the recording item in place. */
	get runningItemId(): string | null {
		return data?.running?.item_id ?? null;
	},
	/** Ticking elapsed seconds of the running timer; `null` when idle. */
	get elapsedSeconds(): number | null {
		const running = data?.running ?? null;
		if (running === null) return null;
		return anchoredElapsedSeconds(running.elapsed_seconds, anchorMs, nowMs);
	},
	get panelOpen(): boolean {
		return panelOpen;
	},
	set panelOpen(open: boolean) {
		panelOpen = open;
	},
	get toast(): TimerToast | null {
		return toast;
	},
	get busy(): boolean {
		return busy;
	},

	/** Fetch once; concurrent callers share the in-flight request. */
	load(): Promise<void> {
		loadPromise ??= fetchState();
		return loadPromise;
	},
	/** Force a refetch — the timer-named live-update event lands here. */
	reload(): Promise<void> {
		loadPromise = fetchState();
		return loadPromise;
	},

	async start(item: string, confirmed = false): Promise<StartResult> {
		busy = true;
		const result = await api.startTimer(item, confirmed);
		busy = false;
		if (result.data === undefined) {
			return { error: result.error ?? 'Starting the timer failed.' };
		}
		if (result.data.outcome === 'needs_confirmation') {
			return 'needs_confirmation';
		}
		// A start is a timer action: it replaces (clears) the toast.
		toast = null;
		apply(result.data.timer);
		return 'started';
	},

	async stop(): Promise<void> {
		busy = true;
		const result = await api.stopTimer();
		busy = false;
		if (result.data === undefined) {
			// The write failed (or nothing was running) — the timer, if
			// any, is still running server-side; only the toast changes.
			toast = { kind: 'stop_failed', message: result.error ?? 'Stopping the timer failed.' };
			return;
		}
		toast = { kind: 'stopped', result: result.data };
		if (data !== null) {
			data = { ...data, running: null };
		}
		panelOpen = false;
	},

	/** Revert the stop's write: put the exact before-value back, or unset
	 * the field when it was absent before. A plain field write through
	 * the existing edit endpoint — no server-side undo memory. */
	async undo(): Promise<void> {
		if (toast === null || (toast.kind !== 'stopped' && toast.kind !== 'undo_failed')) return;
		const stopped = toast.result;
		if (stopped.write === null) return;
		const previous = stopped.write.previous_value;
		busy = true;
		const result = await api.setField(
			stopped.item_id,
			stopped.field,
			previous !== null && previous !== undefined
				? { op: 'replace', value: previous }
				: { op: 'unset' }
		);
		busy = false;
		if (result.error !== undefined) {
			toast = { kind: 'undo_failed', result: stopped, message: result.error };
			return;
		}
		toast = { kind: 'undone', result: stopped };
	},

	dismissToast(): void {
		toast = null;
	}
};
