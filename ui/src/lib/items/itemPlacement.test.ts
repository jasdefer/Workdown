import { afterEach, describe, expect, it } from 'vitest';
import {
	DEFAULT_PLACEMENT,
	loadPlacement,
	otherPlacement,
	placementOf,
	savePlacement,
	switchLabel
} from './itemPlacement';

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

describe('placementOf', () => {
	it('reads the two known values', () => {
		expect(placementOf('dialog')).toBe('dialog');
		expect(placementOf('side')).toBe('side');
	});

	it('falls back to the modal for anything else', () => {
		expect(placementOf(null)).toBe(DEFAULT_PLACEMENT);
		expect(placementOf('')).toBe(DEFAULT_PLACEMENT);
		expect(placementOf('left')).toBe(DEFAULT_PLACEMENT);
		expect(placementOf('SIDE')).toBe(DEFAULT_PLACEMENT);
	});
});

describe('otherPlacement', () => {
	it('flips between the two', () => {
		expect(otherPlacement('dialog')).toBe('side');
		expect(otherPlacement('side')).toBe('dialog');
	});
});

describe('switchLabel', () => {
	it('names where the item goes, not where it is', () => {
		expect(switchLabel('dialog')).toBe('Dock to side');
		expect(switchLabel('side')).toBe('Open as dialog');
	});
});

describe('loadPlacement / savePlacement', () => {
	it('is the modal with no storage at all', () => {
		expect(loadPlacement()).toBe('dialog');
		expect(() => {
			savePlacement('side');
		}).not.toThrow();
	});

	it('is the modal with nothing stored', () => {
		installStorageStub();
		expect(loadPlacement()).toBe('dialog');
	});

	it('round-trips through one project-wide key', () => {
		const backing = installStorageStub();
		savePlacement('side');
		expect(backing.size).toBe(1);
		expect(loadPlacement()).toBe('side');
		savePlacement('dialog');
		expect(loadPlacement()).toBe('dialog');
	});

	it('ignores a stale or foreign stored value', () => {
		const backing = installStorageStub();
		backing.set('workdown.itemPlacement', 'overlay');
		expect(loadPlacement()).toBe('dialog');
	});

	it('survives a storage that throws', () => {
		(globalThis as Record<string, unknown>).localStorage = {
			getItem: () => {
				throw new Error('blocked');
			},
			setItem: () => {
				throw new Error('blocked');
			}
		};
		expect(loadPlacement()).toBe('dialog');
		expect(() => {
			savePlacement('side');
		}).not.toThrow();
	});
});
