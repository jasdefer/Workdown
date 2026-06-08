// Theme store — binary light/dark, persisted to localStorage.
//
// The styling side already works the moment `<html data-theme="...">`
// flips: tokens.css redefines every `--color-*` under
// `[data-theme="dark"]`, and every `var(--color-*)` descendant
// re-renders.
//
// First-paint flicker (FOUC) for users who picked dark is handled by
// an inline script in app.html that runs synchronously before
// stylesheets parse. This module owns runtime state + persistence,
// not the initial paint.

export type Theme = 'light' | 'dark';

const STORAGE_KEY = 'theme';

function readInitial(): Theme {
	if (typeof localStorage === 'undefined') return 'light';
	return localStorage.getItem(STORAGE_KEY) === 'dark' ? 'dark' : 'light';
}

let current = $state<Theme>(readInitial());

$effect.root(() => {
	$effect(() => {
		if (typeof document !== 'undefined') {
			document.documentElement.dataset.theme = current;
		}
		if (typeof localStorage !== 'undefined') {
			localStorage.setItem(STORAGE_KEY, current);
		}
	});
});

export const themeStore = {
	get value(): Theme {
		return current;
	},
	set(next: Theme) {
		current = next;
	},
	toggle() {
		current = current === 'light' ? 'dark' : 'light';
	}
};
