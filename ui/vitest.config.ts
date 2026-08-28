import { defineConfig } from 'vitest/config';

// Standalone Vitest config (no SvelteKit plugin). Every test targets a
// pure, DOM-free module — filter clauses, gantt scale math, timer math,
// formatting — so a plain Node environment is all that's needed. A test
// that needs a DOM or the Svelte compiler (a `.svelte.ts` store, a
// component) would need an environment and the Svelte plugin added here
// first; that is deliberately not set up until such a test exists.
export default defineConfig({
	test: {
		include: ['src/**/*.test.ts'],
		environment: 'node'
	}
});
