import { describe, it, expect } from 'vitest';
import { pillModel, pullMessage } from './gitPill';
import type { GitStatus } from '$lib/api/generated/GitStatus';

const ready = (overrides: Partial<Extract<GitStatus, { state: 'ready' }>> = {}): GitStatus => ({
	state: 'ready',
	branch: 'main',
	has_upstream: true,
	ahead: 0,
	behind: 0,
	dirty_count: 0,
	fetch_error: null,
	...overrides
});

describe('pillModel', () => {
	it('hides the widget until a ready status arrives', () => {
		expect(pillModel(null, false).visible).toBe(false);
		expect(pillModel({ state: 'disabled' }, false).visible).toBe(false);
		expect(pillModel({ state: 'not_a_repo' }, false).visible).toBe(false);
	});

	it('summarises a clean synced repo as in sync', () => {
		const model = pillModel(ready(), false);
		expect(model.visible).toBe(true);
		expect(model.branch).toBe('main');
		expect(model.summary).toBe('in sync');
	});

	it('summarises counts, mentioning only what is non-zero', () => {
		expect(pillModel(ready({ behind: 2 }), false).summary).toBe('↓2');
		expect(pillModel(ready({ ahead: 1 }), false).summary).toBe('↑1');
		expect(pillModel(ready({ ahead: 1, behind: 2, dirty_count: 3 }), false).summary).toBe(
			'↓2 ↑1 · 3 local'
		);
		expect(pillModel(ready({ dirty_count: 1 }), false).summary).toBe('1 local');
	});

	it('enables pull only with an upstream, a clean tree, and no operation running', () => {
		expect(pillModel(ready(), false).canPull).toBe(true);
		expect(pillModel(ready(), true).canPull).toBe(false);
		expect(pillModel(ready({ has_upstream: false }), false).canPull).toBe(false);
		// Pull never touches uncommitted work — the button goes off and
		// the tooltip carries the way out.
		const dirty = pillModel(ready({ dirty_count: 1 }), false);
		expect(dirty.canPull).toBe(false);
		expect(dirty.pullTitle).toBe(
			'Commit your local changes first — pull never touches uncommitted work'
		);
		expect(pillModel(ready(), false).pullTitle).toBe('Pull the latest changes from the remote');
	});

	it('surfaces a failed remote contact as a retryable hint', () => {
		expect(pillModel(ready(), false).remoteHint).toBe(null);
		const unreachable = pillModel(ready({ fetch_error: 'could not resolve host' }), false);
		expect(unreachable.visible).toBe(true);
		expect(unreachable.remoteHint).toBe('could not resolve host');
	});

	it('enables push only with an upstream and commits to publish', () => {
		expect(pillModel(ready({ ahead: 1 }), false).canPush).toBe(true);
		expect(pillModel(ready(), false).canPush).toBe(false);
		expect(pillModel(ready({ ahead: 1 }), true).canPush).toBe(false);
		expect(pillModel(ready({ ahead: 1, has_upstream: false }), false).canPush).toBe(false);
	});

	it('explains why push is unavailable', () => {
		expect(pillModel(ready(), false).pushTitle).toBe('Nothing to push — no local commits');
		expect(pillModel(ready({ has_upstream: false }), false).pushTitle).toBe(
			'No upstream branch configured'
		);
		expect(pillModel(ready({ ahead: 2 }), false).pushTitle).toBe('Push 2 commits');
		expect(pillModel(ready({ ahead: 1 }), false).pushTitle).toBe('Push 1 commit');
	});

	it('tells whether the pull actually brought something in', () => {
		expect(pullMessage(0)).toBe('Already up to date');
		expect(pullMessage(1)).toBe('Pulled 1 commit');
		expect(pullMessage(3)).toBe('Pulled 3 commits');
	});

	it('reminds about uncommitted changes without blocking push', () => {
		const model = pillModel(ready({ ahead: 1, dirty_count: 2 }), false);
		expect(model.canPush).toBe(true);
		expect(model.dirtyHint).toBe('2 uncommitted files stay local — commit them to publish');
		expect(pillModel(ready({ dirty_count: 1 }), false).dirtyHint).toBe(
			'1 uncommitted file stays local — commit it to publish'
		);
		expect(pillModel(ready(), false).dirtyHint).toBe(null);
	});
});
