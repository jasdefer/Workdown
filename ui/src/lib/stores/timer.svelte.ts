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
//
// The announcements live here as well: the store is the one place that
// sees every tick, so it feeds the crossing detector, derives the
// document title, and keeps the deadline worker aimed at the running
// countdown's zero (see `announcements.ts` and `announcer.ts`).

import { api } from '$lib/api/client';
import type { TimerMode } from '$lib/api/generated/TimerMode';
import type { TimerState } from '$lib/api/generated/TimerState';
import type { TimerStopResult } from '$lib/api/generated/TimerStopResult';
import {
	countdownKey,
	createCrossingDetector,
	tabTitle,
	type CountdownObservation
} from '$lib/timer/announcements';
import { announceCrossing, requestNotificationPermission } from '$lib/timer/announcer';
import { stopFailure, undoMutation } from '$lib/timer/stopOutcome';
import { anchoredElapsedSeconds } from '$lib/timer/timerMath';
import { prettifyId } from '$lib/views/prettify';

/** What the single toast slot shows. `stopped` carries the whole stop
 * result (the write half is what undo needs); the failure kinds keep the
 * server's one-line error. `timerStillRunning` decides the advice line:
 * a failed write leaves the interval running ("stop again"), but a 409
 * means there was no work interval to stop — advising another stop
 * there would loop the same refusal. */
export type TimerToast =
	| { kind: 'stopped'; result: TimerStopResult }
	| { kind: 'stop_failed'; message: string; timerStillRunning: boolean }
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
// The split button's selection before a start — local UI state; the
// server remembers what was actually started (`last_mode`), which is
// the default whenever nothing was selected in this tab.
let selectedMode = $state<TimerMode | null>(null);
let loadPromise: Promise<void> | null = null;

function localNow(): number {
	return typeof performance !== 'undefined' ? performance.now() : 0;
}

// The countdown this tab is watching right now — the phase's identity
// plus its ticking remaining seconds; `null` when nothing counts toward
// zero (idle, or a stopwatch session). Feeds the crossing detector, the
// document title and the deadline worker.
function currentCountdown(): CountdownObservation | null {
	const phase = data?.phase;
	if (phase === undefined || phase.phase === 'idle') return null;
	const key = countdownKey(phase);
	if (key === null || phase.phase_length_seconds === null) return null;
	const elapsed = anchoredElapsedSeconds(phase.elapsed_seconds, anchorMs, nowMs);
	return { key, kind: phase.phase, remainingSeconds: phase.phase_length_seconds - elapsed };
}

// The deadline worker: announcements need the exact zero even in a
// background tab, where main-thread timers are throttled to once a
// minute. Created on the first armed countdown, then kept; its 'due'
// ping is just a fresh tick, so the crossing detector and the title
// react exactly as on any other second.
let deadlineWorker: Worker | null = null;

function armDeadline(delayMs: number | null): void {
	if (typeof Worker === 'undefined') return;
	if (deadlineWorker === null) {
		if (delayMs === null) return;
		deadlineWorker = new Worker(new URL('../timer/deadlineWorker.ts', import.meta.url), {
			type: 'module'
		});
		deadlineWorker.onmessage = () => {
			nowMs = localNow();
		};
	}
	deadlineWorker.postMessage(delayMs);
}

function apply(state: TimerState): void {
	data = state;
	anchorMs = localNow();
	nowMs = anchorMs;
}

async function fetchState(): Promise<boolean> {
	const result = await api.getTimer();
	if (result.data === undefined) {
		return false;
	}
	apply(result.data);
	return true;
}

// One fetch at a time, shared by every caller — but never a cached
// failure: a fetch that brought no state clears itself so the next
// `load()` retries, instead of pinning a dead answer (and with it no
// timer UI) on the tab for its whole lifetime.
function beginFetch(): Promise<void> {
	const attempt = fetchState().then((loaded) => {
		if (!loaded && loadPromise === attempt) {
			loadPromise = null;
		}
	});
	loadPromise = attempt;
	return attempt;
}

// Tick once a second while a timer runs; idle otherwise. `$effect.root`
// because this module outlives any component (same pattern as the theme
// store).
$effect.root(() => {
	$effect(() => {
		const phase = data?.phase ?? null;
		if (phase === null || phase.phase === 'idle') return undefined;
		const interval = setInterval(() => {
			nowMs = localNow();
		}, 1000);
		return () => {
			clearInterval(interval);
		};
	});

	// Announce a countdown reaching zero — the one moment the timer is
	// allowed to interrupt. Detection is per-tab (live crossings only);
	// the chime and the notification are claimed across tabs by the
	// announcer. The title needs no announcing — it is derived.
	const detectCrossing = createCrossingDetector();
	$effect(() => {
		const crossing = detectCrossing(currentCountdown());
		if (crossing === null) return;
		const phase = data?.phase;
		const itemName = phase?.phase === 'work' ? prettifyId(phase.item_id) : null;
		void announceCrossing(crossing, itemName, () => {
			panelOpen = true;
		});
	});

	// Keep the worker's alarm aimed at the running countdown's zero.
	// Depends on the server's answer alone, not the ticking clock, so
	// it re-arms once per state change; past zero there is nothing left
	// to aim at.
	$effect(() => {
		const phase = data?.phase;
		if (phase === undefined || phase.phase === 'idle' || phase.phase_length_seconds === null) {
			armDeadline(null);
			return;
		}
		const remainingMs = (phase.phase_length_seconds - phase.elapsed_seconds) * 1000;
		armDeadline(remainingMs > 0 ? remainingMs : null);
	});
});

export const timerStore = {
	get state(): TimerState | null {
		return data;
	},
	/** The id of the item being timed; `null` when no work phase runs
	 * (idle, or a break — a break times no item). What the views compare
	 * against to mark the recording item in place. */
	get runningItemId(): string | null {
		return data?.phase.phase === 'work' ? data.phase.item_id : null;
	},
	/** Ticking elapsed seconds of the running phase — work or break;
	 * `null` when idle. */
	get elapsedSeconds(): number | null {
		const phase = data?.phase;
		if (phase === undefined || phase.phase === 'idle') return null;
		return anchoredElapsedSeconds(phase.elapsed_seconds, anchorMs, nowMs);
	},
	/** What the browser tab says: the pomodoro countdown while one
	 * runs, the alarm form past zero, the plain name otherwise. The
	 * root layout binds this to the document title, so a background
	 * tab is glanceable before zero, not only at it. */
	get documentTitle(): string {
		return tabTitle(currentCountdown());
	},
	/** The mode the split button would start: this tab's selection, or
	 * the server's sticky last-started mode. */
	get startMode(): TimerMode {
		return selectedMode ?? data?.last_mode ?? 'stopwatch';
	},
	set startMode(mode: TimerMode) {
		selectedMode = mode;
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

	/** Fetch once; concurrent callers share the in-flight request. A
	 * failed fetch is not cached — the next caller retries. */
	load(): Promise<void> {
		return loadPromise ?? beginFetch();
	},
	/** Force a refetch — the timer-named live-update event lands here. */
	reload(): Promise<void> {
		return beginFetch();
	},

	async start(item: string, mode: TimerMode, confirmed = false): Promise<StartResult> {
		// The notifications item's permission ask, tied to a pomodoro
		// start: a user gesture, and the moment the permission starts
		// buying something. Prompts only while the answer is undecided;
		// the start proceeds either way.
		if (mode === 'pomodoro') {
			requestNotificationPermission();
		}
		busy = true;
		const result = await api.startTimer(item, mode, confirmed);
		busy = false;
		if (result.data === undefined) {
			return { error: result.error ?? 'Starting the timer failed.' };
		}
		if (result.data.outcome === 'needs_confirmation') {
			return 'needs_confirmation';
		}
		// A start is a timer action: it replaces (clears) the toast. The
		// server now remembers the started mode, so the local selection
		// has served its purpose.
		toast = null;
		selectedMode = null;
		apply(result.data.timer);
		return 'started';
	},

	async stop(): Promise<void> {
		busy = true;
		const result = await api.stopTimer();
		busy = false;
		if (result.data === undefined) {
			// A failed write (or no response at all) leaves the interval
			// running server-side. A `409` is the other family: there was
			// no work interval to stop — this tab's state is stale, so
			// resync it alongside the toast. Which of the two this is, and
			// what the toast says about it, is `stopFailure`'s call.
			const failure = stopFailure(result.status, result.error);
			toast = {
				kind: 'stop_failed',
				message: failure.message,
				timerStillRunning: !failure.nothingToStop
			};
			if (failure.nothingToStop) {
				await this.reload();
			}
			return;
		}
		toast = { kind: 'stopped', result: result.data };
		panelOpen = false;
		// Where the stop landed is the server's decision — idle after a
		// stopwatch session, a counting break after a pomodoro one — so
		// the new state is fetched rather than guessed.
		await this.reload();
	},

	/** End a running break: back to idle. Nothing was written, so there
	 * is no toast — nothing needs reporting or taking back. */
	async endBreak(): Promise<void> {
		busy = true;
		const result = await api.endBreak();
		busy = false;
		panelOpen = false;
		if (result.data !== undefined) {
			apply(result.data);
			return;
		}
		// The break was already gone — another tab ended it or started
		// the next interval. Resync instead of reporting.
		await this.reload();
	},

	/** Revert the stop's write: put the exact before-value back, or unset
	 * the field when it was absent before. A plain field write through
	 * the existing edit endpoint — no server-side undo memory. */
	async undo(): Promise<void> {
		if (toast === null || (toast.kind !== 'stopped' && toast.kind !== 'undo_failed')) return;
		const stopped = toast.result;
		if (stopped.write === null) return;
		busy = true;
		const result = await api.setField(stopped.item_id, stopped.field, undoMutation(stopped.write));
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
