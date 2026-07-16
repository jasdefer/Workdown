import { afterEach, describe, expect, it } from 'vitest';
import {
	displayOverrideParam,
	isEmptyOverride,
	loadDisplayOverride,
	saveDisplayOverride
} from './displayOverride';

// Node has no localStorage — install a minimal stub on globalThis.
function installStorageStub(): Map<string, string> {
	const backing = new Map<string, string>();
	(globalThis as Record<string, unknown>).localStorage = {
		getItem: (key: string) => backing.get(key) ?? null,
		setItem: (key: string, value: string) => void backing.set(key, value),
		removeItem: (key: string) => void backing.delete(key)
	};
	return backing;
}

afterEach(() => {
	delete (globalThis as Record<string, unknown>).localStorage;
});

describe('isEmptyOverride', () => {
	it('treats no roles and empty fields as empty', () => {
		expect(isEmptyOverride({})).toBe(true);
		expect(isEmptyOverride({ fields: [] })).toBe(true);
	});

	it('treats any set role as non-empty', () => {
		expect(isEmptyOverride({ title: 'status' })).toBe(false);
		expect(isEmptyOverride({ subtitle: 'status' })).toBe(false);
		expect(isEmptyOverride({ fields: ['id'] })).toBe(false);
	});
});

describe('displayOverrideParam', () => {
	it('serializes only set roles', () => {
		expect(displayOverrideParam({ title: 'status', fields: [] })).toBe('{"title":"status"}');
		expect(displayOverrideParam({ fields: ['id', 'status'] })).toBe('{"fields":["id","status"]}');
	});
});

describe('load / save round-trip', () => {
	it('round-trips an override per view id', () => {
		installStorageStub();
		saveDisplayOverride('my-view', { title: 'status', fields: ['id'] });
		expect(loadDisplayOverride('my-view')).toEqual({ title: 'status', fields: ['id'] });
		expect(loadDisplayOverride('other-view')).toBeNull();
	});

	it('saving an empty override removes the stored entry', () => {
		const backing = installStorageStub();
		saveDisplayOverride('my-view', { title: 'status' });
		saveDisplayOverride('my-view', {});
		expect(backing.size).toBe(0);
		expect(loadDisplayOverride('my-view')).toBeNull();
	});

	it('tolerates malformed stored JSON', () => {
		const backing = installStorageStub();
		backing.set('workdown.display.my-view', '{not json');
		expect(loadDisplayOverride('my-view')).toBeNull();
	});

	it('returns null without localStorage (no stub installed)', () => {
		expect(loadDisplayOverride('my-view')).toBeNull();
	});
});
