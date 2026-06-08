// Flat-config ESLint setup for the Workdown UI.
//
// Pairs typescript-eslint's strict-type-checked preset with
// eslint-plugin-svelte's recommended set, then adds Prettier last so
// stylistic rules don't fight the formatter. Type-aware rules are wired
// via `projectService: true`, which lets typescript-eslint pick up the
// project's tsconfig automatically.

import js from '@eslint/js';
import typescriptEslint from 'typescript-eslint';
import svelte from 'eslint-plugin-svelte';
import svelteParser from 'svelte-eslint-parser';
import prettier from 'eslint-config-prettier';
import globals from 'globals';

// `*.ts` files (incl. `vite.config.ts`) are already in the SvelteKit-
// generated tsconfig and are picked up by the project service. The
// `*.js` config files (eslint.config.js, svelte.config.js) and
// `vitest.config.ts` sit outside that include list and need
// allowDefaultProject coverage. (`vitest.config.ts` is listed by name
// rather than `*.config.ts`, which would collide with the in-project
// `vite.config.ts`.)
const projectService = {
	allowDefaultProject: ['*.js', '*.config.js', 'vitest.config.ts']
};

export default [
	{
		ignores: [
			'.svelte-kit/**',
			'dist/**',
			'build/**',
			'node_modules/**',
			'src/lib/api/generated/**'
		]
	},
	js.configs.recommended,
	...typescriptEslint.configs.strictTypeChecked,
	...typescriptEslint.configs.stylisticTypeChecked,
	{
		languageOptions: {
			globals: { ...globals.browser, ...globals.node },
			parserOptions: {
				projectService,
				extraFileExtensions: ['.svelte']
			}
		}
	},
	...svelte.configs['flat/recommended'],
	{
		files: ['**/*.svelte'],
		languageOptions: {
			parser: svelteParser,
			parserOptions: {
				parser: typescriptEslint.parser,
				projectService,
				extraFileExtensions: ['.svelte']
			}
		}
	},
	...svelte.configs['flat/prettier'],
	prettier,
	{
		// Project-wide overrides go here as the codebase grows.
		rules: {}
	}
];
