---
id: ui-toolchain-upgrade
title: Major upgrades of the web UI's lint and SvelteKit toolchain
status: to_do
parent: misc-work
tags: [tech-debt]
---

## In plain words

`npm ci` in the UI gate reports 18 advisories (4 low, 14 moderate) after
the patch-level fixes of 2026-11-02. None of them is in the shipped
bundle: `npm audit --omit=dev` is clean. They all come from three
transitive packages the current majors of our build tools pin:

- `minimatch` < 11.0.1 (moderate, globbing denial of service), pulled
  in by ESLint 9 and typescript-eslint 8.
- `deepmerge` 4.x (low, prototype poisoning), pulled in by
  `@sveltejs/vite-plugin-svelte`.
- SvelteKit < 3 (two low advisories about enhanced form redirects and
  prerendered cache headers; the UI uses neither — the server is Rust
  and the app is a static bundle).

`npm audit fix --force` is not the answer: it would *downgrade*
ESLint to 4.0.0, eslint-plugin-svelte to 1.1.2 and typescript-eslint
to 8.14.0 to escape the advisories. The real fix is the next majors.

## Requirement

Upgrade the toolchain in two steps, each one PR, each leaving
`cargo xtask build-ui` green:

1. **Lint stack:** ESLint 10, `@eslint/js` 10, `eslint-plugin-svelte` 3,
   `svelte-eslint-parser` 1, `eslint-config-prettier` 10, `globals` 17,
   `prettier-plugin-svelte` 4. Expect rule renames and a changed flat
   config; the codebase has to lint clean without blanket disables.
2. **SvelteKit 3 and `@sveltejs/adapter-static` 4** once the Svelte 5
   codebase is confirmed supported, with the migration notes read first.

Also worth a look while there: `marked` 18, `cytoscape-dagre` 4 and
TypeScript 7 are a major behind; take them only if they come for free.

Afterwards `npm audit` should be clean in full, and the UI gate could
add `npm audit --omit=dev --audit-level=high` so a shipped advisory
turns the build red (CI checks `cargo audit` today, nothing for npm).
