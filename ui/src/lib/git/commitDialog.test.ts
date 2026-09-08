import { describe, it, expect } from 'vitest';
import { checklist, commitToast, groupFiles, splitDetails } from './commitDialog';
import type { GitCommitResult } from '$lib/api/generated/GitCommitResult';
import type { GitStatus } from '$lib/api/generated/GitStatus';

const readyStatus: GitStatus = {
	state: 'ready',
	branch: 'feature',
	has_upstream: true,
	ahead: 0,
	behind: 0,
	dirty_items: 0,
	dirty_definitions: [],
	fetch_error: null
};

const result = (overrides: Partial<GitCommitResult> = {}): GitCommitResult => ({
	commit: 'a1b2c3d',
	pull: { outcome: 'skipped', reason: 'nothing to integrate — the branch is not behind' },
	push: { outcome: 'pushed', published: false },
	status: readyStatus,
	...overrides
});

describe('groupFiles', () => {
	it('splits work items from definition files', () => {
		const groups = groupFiles([
			{ path: 'workdown-items/a.md', role: 'items', change: 'modified', label: 'edited' },
			{ path: '.workdown/schema.yaml', role: 'schema', change: 'modified', label: 'edited' },
			{ path: 'workdown-items/b.md', role: 'items', change: 'added', label: 'added' }
		]);
		expect(groups.items.map((file) => file.path)).toEqual([
			'workdown-items/a.md',
			'workdown-items/b.md'
		]);
		expect(groups.definitions.map((file) => file.role)).toEqual(['schema']);
	});
});

describe('splitDetails', () => {
	it('separates the sentence from git output after a blank line', () => {
		expect(splitDetails('the commit failed\n\nfatal: something')).toEqual({
			sentence: 'the commit failed',
			details: 'fatal: something'
		});
		expect(splitDetails('nothing to commit')).toEqual({
			sentence: 'nothing to commit',
			details: null
		});
		expect(splitDetails('the commit failed\n\n   ')).toEqual({
			sentence: 'the commit failed',
			details: null
		});
	});
});

describe('checklist', () => {
	it('reports the happy path as three done rows', () => {
		const rows = checklist(result({ pull: { outcome: 'pulled', commits: 2 } }));
		expect(rows.map((row) => [row.label, row.state])).toEqual([
			['Committed a1b2c3d', 'done'],
			['Pulled 2 commits', 'done'],
			['Pushed', 'done']
		]);
	});

	it('frames a skipped pull with the server reason, and names a publish', () => {
		const rows = checklist(result({ push: { outcome: 'pushed', published: true } }));
		expect(rows[1]).toEqual({
			label: 'Pull skipped — nothing to integrate — the branch is not behind',
			state: 'skipped',
			details: null
		});
		expect(rows[2]?.label).toBe('Published feature');
	});

	it('carries a stopped step with its reason and details', () => {
		const rows = checklist(
			result({
				pull: {
					outcome: 'stopped',
					reason: 'the commit is safe and local, but …',
					details: 'CONFLICT (content): Merge conflict in workdown-items/a.md'
				},
				push: { outcome: 'skipped', reason: 'not attempted — the pull did not complete' }
			})
		);
		expect(rows[1]).toEqual({
			label: 'Pull stopped — the commit is safe and local, but …',
			state: 'stopped',
			details: 'CONFLICT (content): Merge conflict in workdown-items/a.md'
		});
		expect(rows[2]).toEqual({
			label: 'Push not attempted — the pull did not complete',
			state: 'skipped',
			details: null
		});
	});

	it('reports a rejected push', () => {
		const rows = checklist(
			result({ push: { outcome: 'stopped', reason: 'push failed', details: 'rejected' } })
		);
		expect(rows[2]).toEqual({
			label: 'Push stopped — push failed',
			state: 'stopped',
			details: 'rejected'
		});
	});
});

describe('commitToast', () => {
	it('says how far the commit got', () => {
		expect(commitToast(result())).toEqual({ kind: 'ok', text: 'Committed and pushed' });
		expect(commitToast(result({ push: { outcome: 'pushed', published: true } }))).toEqual({
			kind: 'ok',
			text: 'Committed and published feature'
		});
		expect(
			commitToast(
				result({
					pull: { outcome: 'stopped', reason: 'x', details: null },
					push: { outcome: 'skipped', reason: 'y' }
				})
			)
		).toEqual({ kind: 'error', text: 'Committed a1b2c3d — the pull stopped; see the pill' });
		expect(
			commitToast(result({ push: { outcome: 'stopped', reason: 'x', details: null } }))
		).toEqual({ kind: 'error', text: 'Committed a1b2c3d — the push stopped; see the pill' });
	});
});
