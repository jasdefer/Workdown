// The deadline alarm — a dedicated worker whose only job is to say
// "the countdown's zero is now" at the right moment. It exists because
// background tabs throttle main-thread timers to once a minute after a
// few minutes hidden — precisely the tab this feature is for — while
// worker timers are exempt.
//
// Protocol: the store posts the milliseconds until zero (re-arming
// replaces any pending alarm), or `null` to disarm; the worker posts
// back a single `'due'` when the moment arrives.

let pending: ReturnType<typeof setTimeout> | null = null;

// The DOM typings see `self` as a window, whose `postMessage` wants a
// target origin; a worker's takes the message alone.
const workerScope = self as unknown as { postMessage(message: string): void };

self.onmessage = (event: MessageEvent<number | null>) => {
	if (pending !== null) {
		clearTimeout(pending);
		pending = null;
	}
	if (event.data === null) return;
	pending = setTimeout(
		() => {
			pending = null;
			workerScope.postMessage('due');
		},
		Math.max(0, event.data)
	);
};
