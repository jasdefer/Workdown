// Pure projection from the server's GitStatus (plus the store's busy
// flag) to everything the header pill renders — kept out of the
// component so the display rules are unit-testable.

import type { GitStatus } from '$lib/api/generated/GitStatus';
import { pluralize } from '$lib/views/format';

export interface GitPillModel {
	visible: boolean;
	branch: string;
	/** One glanceable phrase: `in sync`, or the non-zero counts —
	 * `↓behind ↑ahead · N local`. */
	summary: string;
	canPull: boolean;
	canPush: boolean;
	/** Tooltip for the pull button: the action, or why it is unavailable. */
	pullTitle: string;
	/** Tooltip for the push button: the action, or why it is unavailable. */
	pushTitle: string;
	/** Reminder that uncommitted edits are not published by push;
	 * `null` when the tree is clean. */
	dirtyHint: string | null;
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
	pushTitle: '',
	dirtyHint: null,
	remoteHint: null
};

/** The pull toast: whether anything actually came in, and how much. */
export function pullMessage(pulledCommits: number): string {
	if (pulledCommits === 0) return 'Already up to date';
	return `Pulled ${pluralize(pulledCommits, 'commit')}`;
}

export function pillModel(status: GitStatus | null, busy: boolean): GitPillModel {
	if (status?.state !== 'ready') {
		return HIDDEN;
	}

	const parts: string[] = [];
	const arrows = [
		...(status.behind > 0 ? [`↓${String(status.behind)}`] : []),
		...(status.ahead > 0 ? [`↑${String(status.ahead)}`] : [])
	];
	if (arrows.length > 0) parts.push(arrows.join(' '));
	if (status.dirty_count > 0) parts.push(`${String(status.dirty_count)} local`);
	const summary = parts.length > 0 ? parts.join(' · ') : 'in sync';

	// Pull never runs over uncommitted work — no stashing, no chance of
	// conflict markers landing in item files from a browser button. The
	// tooltip carries the way out.
	let pullTitle: string;
	if (!status.has_upstream) {
		pullTitle = 'No upstream branch configured';
	} else if (status.dirty_count > 0) {
		pullTitle = 'Commit your local changes first — pull never touches uncommitted work';
	} else {
		pullTitle = 'Pull the latest changes from the remote';
	}

	let pushTitle: string;
	if (!status.has_upstream) {
		pushTitle = 'No upstream branch configured';
	} else if (status.ahead === 0) {
		pushTitle = 'Nothing to push — no local commits';
	} else {
		pushTitle = `Push ${pluralize(status.ahead, 'commit')}`;
	}

	const dirtyHint =
		status.dirty_count > 0
			? `${pluralize(status.dirty_count, 'uncommitted file')} ${
					status.dirty_count === 1 ? 'stays local — commit it' : 'stay local — commit them'
				} to publish`
			: null;

	return {
		visible: true,
		branch: status.branch,
		summary,
		canPull: status.has_upstream && status.dirty_count === 0 && !busy,
		canPush: status.has_upstream && status.ahead > 0 && !busy,
		pullTitle,
		pushTitle,
		dirtyHint,
		remoteHint: status.fetch_error
	};
}
