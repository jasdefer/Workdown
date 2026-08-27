// Git sync store — the client-side copy of the server's git status,
// driving the header pill.
//
// The server owns the repository; this store holds its last status
// answer and the in-flight/result state of the one operation the pill
// can run at a time. The first load fetches from the remote so
// `behind` starts truthful; afterwards the generic file-change ping
// (wired in the root layout) refreshes the counts locally — every item
// edit, timer write or CLI mutation moves `dirty_count`, and a pull's
// rewritten files land the same way.

import { api } from '$lib/api/client';
import type { GitStatus } from '$lib/api/generated/GitStatus';

export interface GitMessage {
	kind: 'ok' | 'error';
	text: string;
}

let status = $state<GitStatus | null>(null);
let busy = $state(false);
let message = $state<GitMessage | null>(null);
let loadPromise: Promise<void> | null = null;
let messageTimer: ReturnType<typeof setTimeout> | null = null;

function show(next: GitMessage): void {
	if (messageTimer !== null) {
		clearTimeout(messageTimer);
		messageTimer = null;
	}
	message = next;
	// Confirmations fade on their own; an error stays until the next
	// action replaces it (or the user dismisses it) — it explains a
	// button that seemingly did nothing.
	if (next.kind === 'ok') {
		messageTimer = setTimeout(() => {
			message = null;
		}, 6000);
	}
}

async function fetchStatus(withRemote: boolean): Promise<boolean> {
	const result = await api.getGitStatus(withRemote);
	if (result.data === undefined) {
		return false;
	}
	status = result.data;
	return true;
}

// One initial fetch shared by every caller; a failed one clears itself
// so the next `load()` retries instead of pinning a dead answer.
function beginLoad(): Promise<void> {
	const attempt = fetchStatus(true).then((loaded) => {
		if (!loaded && loadPromise === attempt) {
			loadPromise = null;
		}
	});
	loadPromise = attempt;
	return attempt;
}

export const gitStore = {
	get status(): GitStatus | null {
		return status;
	},
	get busy(): boolean {
		return busy;
	},
	get message(): GitMessage | null {
		return message;
	},

	/** Fetch once (remote included); concurrent callers share the request. */
	load(): Promise<void> {
		return loadPromise ?? beginLoad();
	},

	/** Cheap local-only recount — the file-change ping lands here. Skipped
	 * while an operation runs; the operation's answer supersedes it. */
	async refresh(): Promise<void> {
		if (busy || status?.state !== 'ready') return;
		await fetchStatus(false);
	},

	async pull(): Promise<void> {
		busy = true;
		const result = await api.gitPull();
		busy = false;
		if (result.data === undefined) {
			show({ kind: 'error', text: result.error ?? 'Pull failed.' });
			return;
		}
		status = result.data;
		show({ kind: 'ok', text: 'Pulled' });
	},

	async push(): Promise<void> {
		busy = true;
		const result = await api.gitPush();
		busy = false;
		if (result.data === undefined) {
			show({ kind: 'error', text: result.error ?? 'Push failed.' });
			return;
		}
		status = result.data;
		show({ kind: 'ok', text: 'Pushed' });
	},

	dismissMessage(): void {
		message = null;
	}
};
