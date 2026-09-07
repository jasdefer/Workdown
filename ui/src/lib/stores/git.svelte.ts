// Git sync store — the client-side copy of the server's git status,
// driving the header pill.
//
// The server owns the repository; this store holds its last status
// answer and the in-flight/result state of the one operation the pill
// can run at a time. The first load fetches from the remote so
// `behind` starts truthful; afterwards two live-update signals keep it
// current without touching the network: the generic file-change ping
// moves the local counts (item edits, timer writes, CLI mutations), and
// the git-named ping fires when the repository itself moves (a commit
// or fetch in a terminal — the server watches `.git` for exactly this).

import { api } from '$lib/api/client';
import type { GitCommitPreview } from '$lib/api/generated/GitCommitPreview';
import type { GitCommitResult } from '$lib/api/generated/GitCommitResult';
import type { GitStatus } from '$lib/api/generated/GitStatus';
import { commitToast } from '$lib/git/commitDialog';
import { pullMessage, pushMessage } from '$lib/git/gitPill';

export interface GitMessage {
	kind: 'ok' | 'error';
	text: string;
}

let status = $state<GitStatus | null>(null);
let busy = $state(false);
let message = $state<GitMessage | null>(null);
let messageTimer: ReturnType<typeof setTimeout> | null = null;

// The "Commit & push" dialog's state. The dialog is the review moment:
// it opens on a preview (files + generated message) and, once confirmed,
// shows the per-step result until closed. A `409` on confirm means the
// set of changes moved (or the commit was refused for another reason
// the message explains); the preview is reloaded so the list is current
// again, and the user's typed message is kept by the component.
let dialogOpen = $state(false);
let preview = $state<GitCommitPreview | null>(null);
let previewError = $state<string | null>(null);
let commitResult = $state<GitCommitResult | null>(null);
let commitError = $state<string | null>(null);

// Response-ordering guards. `statusGeneration` is bumped whenever an
// operation (pull/push) writes `status` directly, so a slower status
// request started *before* the operation cannot land afterwards and
// overwrite the fresh answer with pre-operation counts.
// `refreshPromise` coalesces concurrent refreshes (SSE pings can stack
// while a status request is still running) into one request.
let statusGeneration = 0;
let refreshPromise: Promise<void> | null = null;
let loadStarted = false;

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

async function fetchStatus(withRemote: boolean): Promise<void> {
	const generation = statusGeneration;
	const result = await api.getGitStatus(withRemote);
	if (result.data === undefined) return;
	if (generation !== statusGeneration) return; // superseded by an operation's answer
	const next = result.data;
	// A local-only answer knows nothing about the remote; keep the last
	// remote attempt's verdict visible instead of silently clearing the
	// "remote not reachable" hint on the next file-change ping.
	if (!withRemote && next.state === 'ready' && status?.state === 'ready') {
		next.fetch_error = status.fetch_error;
	}
	status = next;
}

/** One operation at a time: flips `busy` around `call`, routes the
 * failure or the applied result into the message slot. */
async function runOperation<T>(
	call: () => Promise<{ data?: T; error?: string }>,
	apply: (data: T) => { status: GitStatus; toast: string },
	failureText: string
): Promise<void> {
	busy = true;
	try {
		const result = await call();
		if (result.data === undefined) {
			show({ kind: 'error', text: result.error ?? failureText });
			return;
		}
		const applied = apply(result.data);
		statusGeneration += 1;
		status = applied.status;
		show({ kind: 'ok', text: applied.toast });
	} finally {
		busy = false;
	}
}

async function loadPreview(): Promise<void> {
	const result = await api.getGitCommitPreview();
	if (result.data === undefined) {
		previewError = result.error ?? 'Could not read the changes.';
		preview = null;
		return;
	}
	previewError = null;
	preview = result.data;
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
	get dialogOpen(): boolean {
		return dialogOpen;
	},
	get preview(): GitCommitPreview | null {
		return preview;
	},
	get previewError(): string | null {
		return previewError;
	},
	get commitResult(): GitCommitResult | null {
		return commitResult;
	},
	get commitError(): string | null {
		return commitError;
	},

	/** Open the dialog on a fresh preview. */
	openCommitDialog(): void {
		dialogOpen = true;
		preview = null;
		previewError = null;
		commitResult = null;
		commitError = null;
		void loadPreview();
	},

	/** Ask the server again what the commit would cover — after a stale
	 * refusal, or on the dialog's own reload control. */
	reloadPreview(): Promise<void> {
		return loadPreview();
	},

	closeCommitDialog(): void {
		dialogOpen = false;
	},

	/** Confirm: commit the previewed files with `text`, then pull if
	 * behind and push — one server action. The result stays in the
	 * dialog as a checklist; the toast summarises it for after closing. */
	async commit(text: string): Promise<void> {
		if (busy || preview === null) return;
		busy = true;
		commitError = null;
		try {
			const result = await api.gitCommit({
				message: text,
				files: preview.files.map((file) => file.path)
			});
			if (result.data === undefined) {
				commitError = result.error ?? 'Commit failed.';
				// Refused about repository state — most often the set of
				// changes moved. Whatever the reason, the list should be
				// current when the user reads the message.
				if (result.status === 409) await loadPreview();
				return;
			}
			commitResult = result.data;
			statusGeneration += 1;
			status = result.data.status;
			show(commitToast(result.data));
		} finally {
			busy = false;
		}
	},

	/** The initial fetch, remote included — called once from the root
	 * layout's mount. If it fails outright, the next ping's `refresh()`
	 * retries with a local-only request, so the pill still appears. */
	async load(): Promise<void> {
		loadStarted = true;
		await fetchStatus(true);
	},

	/** Cheap local-only recount — the file-change and git pings land
	 * here. Coalesced while one is in flight; skipped while an operation
	 * runs (its answer supersedes) and for the two stable states that
	 * only a server restart can change. */
	refresh(): Promise<void> {
		const stable = status?.state === 'disabled' || status?.state === 'not_a_repo';
		if (busy || stable || !loadStarted) return Promise.resolve();
		refreshPromise ??= fetchStatus(false).finally(() => {
			refreshPromise = null;
		});
		return refreshPromise;
	},

	/** Contact the remote again — the retry behind the pill's
	 * "remote not reachable" hint. */
	async retryRemote(): Promise<void> {
		if (busy) return;
		await fetchStatus(true);
	},

	pull(): Promise<void> {
		return runOperation(
			() => api.gitPull(),
			(data) => ({ status: data.status, toast: pullMessage(data.pulled_commits) }),
			'Pull failed.'
		);
	},

	push(): Promise<void> {
		return runOperation(
			() => api.gitPush(),
			(data) => ({
				status: data.status,
				toast: pushMessage(data.published, data.status.state === 'ready' ? data.status.branch : '')
			}),
			'Push failed.'
		);
	},

	dismissMessage(): void {
		message = null;
	}
};
