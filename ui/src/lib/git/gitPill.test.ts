import { describe, it, expect } from 'vitest';
import { pillModel, pullMessage, pushMessage } from './gitPill';
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
		const unpublished = pillModel(ready({ has_upstream: false }), false);
		expect(unpublished.canPull).toBe(false);
		expect(unpublished.pullTitle).toBe('Not published yet — nothing to pull from');
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

	it('enables push only with commits to publish and no operation running', () => {
		expect(pillModel(ready({ ahead: 1 }), false).canPush).toBe(true);
		expect(pillModel(ready(), false).canPush).toBe(false);
		expect(pillModel(ready({ ahead: 1 }), true).canPush).toBe(false);
	});

	it('explains why push is unavailable', () => {
		expect(pillModel(ready(), false).pushTitle).toBe('Nothing to push — no local commits');
		expect(pillModel(ready({ ahead: 2 }), false).pushTitle).toBe('Push 2 commits');
		expect(pillModel(ready({ ahead: 1 }), false).pushTitle).toBe('Push 1 commit');
		expect(pillModel(ready({ ahead: 1 }), false).pushLabel).toBe('Push');
	});

	it('turns push into publish on a branch that has no upstream', () => {
		// Nothing has left the machine, so "in sync" would be a lie; the
		// button offers the way out instead of greying out. Ahead/behind
		// carry no information without an upstream, so no arrows.
		const model = pillModel(ready({ branch: 'feature', has_upstream: false }), false);
		expect(model.summary).toBe('not published');
		expect(model.pushLabel).toBe('Publish');
		expect(model.pushTitle).toBe('Publish feature');
		expect(model.canPush).toBe(true);
		expect(pillModel(ready({ has_upstream: false }), true).canPush).toBe(false);
		expect(pillModel(ready({ has_upstream: false, dirty_count: 2 }), false).summary).toBe(
			'not published · 2 local'
		);
	});

	it('offers nothing on a detached head', () => {
		// `HEAD` is the wire contract for "no branch": nothing to publish,
		// nothing to pull from — and again not "in sync".
		const model = pillModel(ready({ branch: 'HEAD', has_upstream: false }), false);
		expect(model.summary).toBe('detached');
		expect(model.pushLabel).toBe('Push');
		expect(model.canPush).toBe(false);
		expect(model.canPull).toBe(false);
		expect(model.pushTitle).toBe('Detached HEAD — check out a branch first');
		expect(model.pullTitle).toBe('Detached HEAD — check out a branch first');
	});

	it('tells whether the pull actually brought something in', () => {
		expect(pullMessage(0)).toBe('Already up to date');
		expect(pullMessage(1)).toBe('Pulled 1 commit');
		expect(pullMessage(3)).toBe('Pulled 3 commits');
	});

	it('tells whether the push published the branch or pushed to its upstream', () => {
		expect(pushMessage(false, 'main')).toBe('Pushed');
		expect(pushMessage(true, 'feature')).toBe('Published feature');
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
