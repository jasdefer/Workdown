import { describe, it, expect } from 'vitest';
import { pillModel } from './gitPill';
import type { GitStatus } from '$lib/api/generated/GitStatus';

const ready = (overrides: Partial<Extract<GitStatus, { state: 'ready' }>> = {}): GitStatus => ({
	state: 'ready',
	branch: 'main',
	has_upstream: true,
	ahead: 0,
	behind: 0,
	dirty_count: 0,
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

	it('enables pull whenever there is an upstream and no operation runs', () => {
		expect(pillModel(ready(), false).canPull).toBe(true);
		expect(pillModel(ready(), true).canPull).toBe(false);
		expect(pillModel(ready({ has_upstream: false }), false).canPull).toBe(false);
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
