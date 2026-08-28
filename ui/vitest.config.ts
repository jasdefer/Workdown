import { fileURLToPath } from 'node:url';
import { defineConfig } from 'vitest/config';

// Standalone Vitest config (no SvelteKit plugin). Every test targets a
// pure, DOM-free module — filter clauses, gantt scale math, timer math,
// formatting — so a plain Node environment is all that's needed. A test
// that needs a DOM or the Svelte compiler (a `.svelte.ts` store, a
// component) would need an environment and the Svelte plugin added here
// first; that is deliberately not set up until such a test exists.
//
// The `$lib` alias is SvelteKit's, and without the plugin nothing here
// knows it: type-only imports are erased before Vitest sees them, but a
// value import of a `$lib` module fails to resolve. Declared once, so a
// module under test can import generated data the same way the app does.
export default defineConfig({
	resolve: {
		alias: {
			$lib: fileURLToPath(new URL('./src/lib', import.meta.url))
		}
	},
	test: {
		include: ['src/**/*.test.ts'],
		environment: 'node'
	}
});
