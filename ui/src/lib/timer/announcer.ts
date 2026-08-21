// The announcer — the noisy half of [[timer-notifications]], where the
// pure crossing from `announcements.ts` becomes a chime and a system
// notification. Browser-facing and deliberately thin: everything worth
// asserting on lives in the pure module; nothing here writes state or
// touches a timer (item decision 4).

import type { CountdownObservation } from './announcements';

/** One localStorage flag per announced phase identity — the cross-tab
 * half of "each crossing is announced exactly once". */
const CLAIM_PREFIX = 'workdown.timer.announced.';
/** The Web Lock serializing rival tabs' claims. */
const CLAIM_LOCK = 'workdown-timer-announce';

/**
 * Claim the right to announce this crossing: true for exactly one tab
 * per phase identity. First tab to flag the key in localStorage wins;
 * the Web Lock makes the check-then-set atomic across tabs (and is
 * skipped where unsupported — rival ticks are a second apart, so the
 * flag alone already decides in practice). Stale flags from past
 * phases are swept here, the one place that touches them.
 */
async function claimAnnouncement(key: string): Promise<boolean> {
	const claim = (): boolean => {
		for (let index = localStorage.length - 1; index >= 0; index -= 1) {
			const stored = localStorage.key(index);
			if (stored !== null && stored.startsWith(CLAIM_PREFIX) && stored !== CLAIM_PREFIX + key) {
				localStorage.removeItem(stored);
			}
		}
		if (localStorage.getItem(CLAIM_PREFIX + key) !== null) return false;
		localStorage.setItem(CLAIM_PREFIX + key, '1');
		return true;
	};
	try {
		// An `in` probe: the DOM typings promise `locks` unconditionally,
		// but older browsers may not deliver it.
		if ('locks' in navigator) {
			let won = false;
			await navigator.locks.request(CLAIM_LOCK, () => {
				won = claim();
			});
			return won;
		}
		return claim();
	} catch {
		// Storage unavailable (private mode quirks): better a doubled
		// chime than a silent zero.
		return true;
	}
}

/**
 * The chime pair (item decision 5): synthesized, no shipped asset.
 * Two tones — descending when the work interval ends (go rest),
 * ascending when the break ends (back to it). A tab that never saw a
 * user interaction keeps a suspended AudioContext and stays silent —
 * the accepted autoplay limitation; the tab that started the timer
 * always has its gesture.
 */
function playChime(kind: 'work' | 'break'): void {
	if (typeof AudioContext === 'undefined') return;
	const context = new AudioContext();
	if (context.state === 'suspended') {
		void context.resume();
	}
	const frequencies = kind === 'work' ? [880, 587.33] : [587.33, 880];
	const toneSeconds = 0.22;
	frequencies.forEach((frequency, position) => {
		const startAt = context.currentTime + position * (toneSeconds + 0.06);
		const oscillator = context.createOscillator();
		oscillator.type = 'sine';
		oscillator.frequency.value = frequency;
		const gain = context.createGain();
		gain.gain.setValueAtTime(0.0001, startAt);
		gain.gain.exponentialRampToValueAtTime(0.28, startAt + 0.02);
		gain.gain.exponentialRampToValueAtTime(0.0001, startAt + toneSeconds);
		oscillator.connect(gain).connect(context.destination);
		oscillator.start(startAt);
		oscillator.stop(startAt + toneSeconds + 0.02);
	});
	setTimeout(() => {
		void context.close();
	}, 1000);
}

/**
 * Ask for notification permission — called on every pomodoro start
 * (a user gesture); the browser only actually prompts while the
 * answer is still 'default', and denial is the opt-out (item
 * decision 6). Nothing awaits the answer: the start proceeds either
 * way.
 */
export function requestNotificationPermission(): void {
	if (typeof Notification === 'undefined') return;
	if (Notification.permission === 'default') {
		void Notification.requestPermission();
	}
}

/** The wording of item decision 8 — an announcement, never a report
 * of a stop that did not happen. */
function notificationText(
	kind: 'work' | 'break',
	itemName: string | null
): { title: string; body: string } {
	if (kind === 'work') {
		return {
			title: 'Interval over',
			body: `${itemName ?? 'The timer'} — still recording until you stop.`
		};
	}
	return { title: 'Break over', body: 'Start the next interval.' };
}

function postNotification(
	kind: 'work' | 'break',
	itemName: string | null,
	onOpen: () => void
): void {
	if (typeof Notification === 'undefined' || Notification.permission !== 'granted') return;
	const text = notificationText(kind, itemName);
	// `tag` collapses rivals into one banner; `silent` keeps the OS
	// sound from doubling the chime (item decision 6).
	const notification = new Notification(text.title, {
		body: text.body,
		tag: 'workdown-timer',
		silent: true
	});
	notification.onclick = () => {
		window.focus();
		onOpen();
		notification.close();
	};
}

/**
 * Announce one live-observed crossing: claim it against rival tabs,
 * then chime and notify. Every tab's title flips regardless — the
 * title is per-tab and handled by the store, not here.
 */
export async function announceCrossing(
	observation: CountdownObservation,
	itemName: string | null,
	onOpen: () => void
): Promise<void> {
	if (!(await claimAnnouncement(observation.key))) return;
	playChime(observation.kind);
	postNotification(observation.kind, itemName, onOpen);
}
