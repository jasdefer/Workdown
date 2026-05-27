import { defineConfig } from 'vitest/config';

// Standalone Vitest config (no SvelteKit plugin) — the only tests so far
// are the pure, DOM-free time-axis helpers in the gantt view, so a plain
// Node environment is all that's needed.
export default defineConfig({
	test: {
		include: ['src/**/*.test.ts'],
		environment: 'node'
	}
});
