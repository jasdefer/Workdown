// The localStorage stand-in for pure-module tests. Vitest runs them in
// Node, which has no `localStorage`, so a module that persists there
// is tested against one of these on `globalThis`: a Map-backed store
// the test can seed and inspect, or one whose every access throws —
// the privacy-mode case a module must survive. `removeStorageStub`
// takes either away again; tests call it from `afterEach` so the next
// file starts with no storage at all, which is itself a case.
//
// Not a test file (the Vitest include is `*.test.ts`), so it is never
// collected on its own.

export function installStorageStub(): Map<string, string> {
	const backing = new Map<string, string>();
	(globalThis as Record<string, unknown>).localStorage = {
		getItem: (key: string) => backing.get(key) ?? null,
		setItem: (key: string, value: string) => void backing.set(key, value),
		removeItem: (key: string) => void backing.delete(key)
	};
	return backing;
}

export function installThrowingStorageStub(): void {
	const blocked = (): never => {
		throw new Error('blocked');
	};
	(globalThis as Record<string, unknown>).localStorage = {
		getItem: blocked,
		setItem: blocked,
		removeItem: blocked
	};
}

export function removeStorageStub(): void {
	delete (globalThis as Record<string, unknown>).localStorage;
}
