// Pure helpers behind the "Commit & push" dialog — what the file list,
// the result checklist and the error box say — kept out of the
// component so the wording is unit-testable.

import type { GitChangedFile } from '$lib/api/generated/GitChangedFile';
import type { GitCommitResult } from '$lib/api/generated/GitCommitResult';
import { pluralize } from '$lib/views/format';

/** The files a commit would cover, split the way the pill counts them:
 * work items, then definition files (schema, views, …). */
export function groupFiles(files: GitChangedFile[]): {
	items: GitChangedFile[];
	definitions: GitChangedFile[];
} {
	return {
		items: files.filter((file) => file.role === 'items'),
		definitions: files.filter((file) => file.role !== 'items')
	};
}

/** A server error is "sentence, blank line, git's raw output" when there
 * is raw output worth keeping behind a toggle; split it so the dialog
 * shows the sentence and folds the rest. */
export function splitDetails(error: string): { sentence: string; details: string | null } {
	const separator = error.indexOf('\n\n');
	if (separator === -1) return { sentence: error, details: null };
	const details = error.slice(separator + 2).trim();
	return { sentence: error.slice(0, separator), details: details.length > 0 ? details : null };
}

export interface ChecklistRow {
	label: string;
	state: 'done' | 'skipped' | 'stopped';
	/** Git's raw output for a stopped step, behind a details toggle. */
	details: string | null;
}

/** The per-step report as three rows: committed, pull, push. The server
 * words every skipped and stopped step; this only frames them. */
export function checklist(result: GitCommitResult): ChecklistRow[] {
	const rows: ChecklistRow[] = [
		{ label: `Committed ${result.commit}`, state: 'done', details: null }
	];

	const pull = result.pull;
	switch (pull.outcome) {
		case 'skipped':
			rows.push({ label: `Pull skipped — ${pull.reason}`, state: 'skipped', details: null });
			break;
		case 'pulled':
			rows.push({
				label: `Pulled ${pluralize(pull.commits, 'commit')}`,
				state: 'done',
				details: null
			});
			break;
		case 'stopped':
			rows.push({
				label: `Pull stopped — ${pull.reason}`,
				state: 'stopped',
				details: pull.details
			});
			break;
	}

	const push = result.push;
	switch (push.outcome) {
		case 'pushed':
			rows.push({
				label: push.published ? `Published ${branchOf(result)}` : 'Pushed',
				state: 'done',
				details: null
			});
			break;
		case 'skipped':
			rows.push({ label: `Push ${push.reason}`, state: 'skipped', details: null });
			break;
		case 'stopped':
			rows.push({
				label: `Push stopped — ${push.reason}`,
				state: 'stopped',
				details: push.details
			});
			break;
	}
	return rows;
}

/** The toast after the dialog closes: what got as far as the remote. */
export function commitToast(result: GitCommitResult): { kind: 'ok' | 'error'; text: string } {
	if (result.push.outcome === 'pushed') {
		return {
			kind: 'ok',
			text: result.push.published
				? `Committed and published ${branchOf(result)}`
				: 'Committed and pushed'
		};
	}
	if (result.pull.outcome === 'stopped') {
		return { kind: 'error', text: `Committed ${result.commit} — the pull stopped; see the pill` };
	}
	return { kind: 'error', text: `Committed ${result.commit} — the push stopped; see the pill` };
}

function branchOf(result: GitCommitResult): string {
	return result.status.state === 'ready' ? result.status.branch : 'the branch';
}
