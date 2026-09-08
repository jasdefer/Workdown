// Pure projection from the server's GitStatus (plus the store's busy
// flag) to everything the header pill renders — kept out of the
// component so the display rules are unit-testable.

import type { GitStatus } from '$lib/api/generated/GitStatus';
import { pluralize } from '$lib/views/format';

export interface GitPillModel {
	visible: boolean;
	branch: string;
	/** One glanceable phrase: `in sync`, or the non-zero counts —
	 * `↓behind ↑ahead · N local`. A branch with no upstream reads `not
	 * published` (never `in sync`: nothing has left the machine), a
	 * detached head `detached`. */
	summary: string;
	canPull: boolean;
	canPush: boolean;
	/** Tooltip for the pull button: the action, or why it is unavailable. */
	pullTitle: string;
	/** The push button's label: `Push` to an existing upstream, `Publish`
	 * when the first push will also create the remote branch — same
	 * slot, same gesture, the label says what will happen. */
	pushLabel: 'Push' | 'Publish';
	/** Tooltip for the push button: the action, or why it is unavailable. */
	pushTitle: string;
	/** Reminder that uncommitted edits are not published by push;
	 * `null` when the tree is clean. */
	dirtyHint: string | null;
	/** The button rule: which of the three buttons are on the pill at
	 * all. Uncommitted workdown changes put "Commit & push" first and
	 * keep Pull (disabled, with its hint) beside it; a clean tree shows
	 * Pull only when behind and Push only when ahead or unpublished; a
	 * clean, synced branch shows no button. */
	showCommit: boolean;
	showPull: boolean;
	showPush: boolean;
	canCommit: boolean;
	commitTitle: string;
	/** The last remote contact's failure, when there was one — shown as
	 * a retry affordance; `null` while the remote answers. */
	remoteHint: string | null;
}

const HIDDEN: GitPillModel = {
	visible: false,
	branch: '',
	summary: '',
	canPull: false,
	canPush: false,
	pullTitle: '',
	pushLabel: 'Push',
	pushTitle: '',
	dirtyHint: null,
	remoteHint: null,
	showCommit: false,
	showPull: false,
	showPush: false,
	canCommit: false,
	commitTitle: ''
};

/** The pull toast: whether anything actually came in, and how much. */
export function pullMessage(pulledCommits: number): string {
	if (pulledCommits === 0) return 'Already up to date';
	return `Pulled ${pluralize(pulledCommits, 'commit')}`;
}

/** The push toast: a first publish names the branch it created on the
 * remote; an ordinary push to an existing upstream just says so. */
export function pushMessage(published: boolean, branch: string): string {
	return published ? `Published ${branch}` : 'Pushed';
}

export function pillModel(status: GitStatus | null, busy: boolean): GitPillModel {
	if (status?.state !== 'ready') {
		return HIDDEN;
	}

	// `HEAD` is the wire contract's name for a detached head: no branch,
	// so nothing to publish or pull.
	const detached = status.branch === 'HEAD';
	const unpublished = !status.has_upstream && !detached;
	// Uncommitted work *inside the workdown paths* — what a commit from
	// here would cover. Dirty files elsewhere in the repository are not
	// the pill's business and never reach it.
	const hasLocal = status.dirty_items > 0 || status.dirty_definitions.length > 0;

	const parts: string[] = [];
	if (detached) {
		parts.push('detached');
	} else if (unpublished) {
		// Ahead/behind are meaningless without an upstream (git has no
		// honest number for "commits the remote lacks" when the remote
		// has no such branch), so the arrows stay off and the phrase
		// says the one thing that matters: this work is only here.
		parts.push('not published');
	} else {
		const arrows = [
			...(status.behind > 0 ? [`↓${String(status.behind)}`] : []),
			...(status.ahead > 0 ? [`↑${String(status.ahead)}`] : [])
		];
		if (arrows.length > 0) parts.push(arrows.join(' '));
	}
	// Items are counted, definition files are named by role — `3 items ·
	// schema` says what changed better than one number over all files.
	if (status.dirty_items > 0) parts.push(pluralize(status.dirty_items, 'item'));
	if (status.dirty_definitions.length > 0) parts.push(status.dirty_definitions.join(', '));
	const summary = parts.length > 0 ? parts.join(' · ') : 'in sync';

	// Pull never runs over uncommitted work — no stashing, no chance of
	// conflict markers landing in item files from a browser button. The
	// tooltip carries the way out.
	let pullTitle: string;
	if (detached) {
		pullTitle = 'Detached HEAD — check out a branch first';
	} else if (unpublished) {
		pullTitle = 'Not published yet — nothing to pull from';
	} else if (hasLocal) {
		pullTitle = 'Commit your local changes first — pull never touches uncommitted work';
	} else {
		pullTitle = 'Pull the latest changes from the remote';
	}

	// Push and publish are one gesture — get my commits onto the remote.
	// The first time, git also creates the remote branch and records it
	// as upstream; the server handles that, the label announces it.
	// Which remote is the server's call at click time (a missing one
	// comes back as an error message), so the button is simply on.
	const pushLabel = unpublished ? 'Publish' : 'Push';
	let pushTitle: string;
	if (detached) {
		pushTitle = 'Detached HEAD — check out a branch first';
	} else if (unpublished) {
		pushTitle = `Publish ${status.branch}`;
	} else if (status.ahead === 0) {
		pushTitle = 'Nothing to push — no local commits';
	} else {
		pushTitle = `Push ${pluralize(status.ahead, 'commit')}`;
	}

	const dirtyHint = hasLocal ? 'Uncommitted changes stay local — commit them to publish' : null;

	// The button rule. A detached head keeps both sync buttons visible
	// and off, so their tooltips can say why.
	const showCommit = hasLocal && !detached;
	const showPull = detached || hasLocal || (status.has_upstream && status.behind > 0);
	const showPush = detached || (!hasLocal && (unpublished || status.ahead > 0));

	return {
		visible: true,
		branch: status.branch,
		summary,
		canPull: status.has_upstream && !hasLocal && !busy,
		canPush: (unpublished || status.ahead > 0) && !busy,
		pullTitle,
		pushLabel,
		pushTitle,
		dirtyHint,
		remoteHint: status.fetch_error,
		showCommit,
		showPull,
		showPush,
		canCommit: showCommit && !busy,
		commitTitle: 'Review the changes, commit them, and push'
	};
}
