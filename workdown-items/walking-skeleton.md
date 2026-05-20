---
id: walking-skeleton
type: issue
status: to_do
title: workdown serve skeleton with embedded UI
parent: server
---

End-to-end smoke test of the build and serve pipeline: `workdown serve` boots axum, the Vite-built Svelte bundle is embedded via `rust-embed`, the browser loads a placeholder page with the app shell wired up. No real API, no real view rendering — but the foundational UI tooling, dev workflow, and embed pipeline all decided and working.

The skeleton is also the right place to lock in tooling-shaped decisions (framework version, CSS strategy, language tooling, dev container layout) because they are painful to retrofit once components exist.

## Scope

### CLI + server

- `workdown serve [--port N] [--open]` (defaults: auto-port, no browser open)
- Startup log line: URL, port, PID
- `GET /` serves embedded `index.html`
- `GET /assets/*` serves the embedded bundle
- SPA fallback on non-`/api/*` unmatched paths returns `index.html` (open question: path-based vs hash-based routing — see below)

### UI toolchain

- `ui/` project scaffolded (Svelte 5 + TypeScript + Vite)
- Tailwind CSS v4 via `@tailwindcss/vite`
- ESLint (`typescript-eslint` strict + `eslint-plugin-svelte`) + Prettier
- `tsconfig.json` strict: `strict`, `noUncheckedIndexedAccess`, `noUnusedLocals`, `noUnusedParameters`, `verbatimModuleSyntax`, `exactOptionalPropertyTypes`, `allowJs: false`
- All source `.ts` / `.svelte` — no `.js` in the source tree (configs included)

### UI app shell

- Header (app name, theme toggle)
- Main content area with a placeholder route
- Theme store (Svelte 5 rune, persisted to `localStorage`)
- Tailwind `darkMode: 'class'` strategy — every component written with `dark:` variants from day one
- Router (open question: `svelte-spa-router` vs hand-rolled — see below)

### Build + embed pipeline

- `cargo build` invokes the Vite production build (open question: `build.rs` vs xtask vs cargo alias — see below)
- `rust-embed` packs `ui/dist/` into the server crate
- Vite's hashed asset filenames referenced correctly from generated `index.html`

### Dev workflow

- `pnpm run dev` runs Vite with HMR on a fixed port
- Vite proxies `/api/*` to a running `workdown serve` on a fixed port
- Documented in repo README or `docs/`
- (open question: port numbers — see below)

### Devcontainer

- Add `ghcr.io/devcontainers/features/node:1` (Node 22 LTS)
- pnpm via corepack in `postCreateCommand`
- VS Code extensions: `svelte.svelte-vscode`, `bradlc.vscode-tailwindcss`, `dbaeumer.vscode-eslint`, `esbenp.prettier-vscode`
- Persistent volume mount for `ui/node_modules` (mirrors the existing `target` volume pattern)

### CI

- `actions/setup-node@v4` (Node 22) alongside the Rust setup
- pnpm cache configured
- CI builds the workspace including the embedded UI

## Acceptance

- Fresh clone → devcontainer rebuild → `cargo build --workspace` → `workdown serve` shows the Svelte placeholder in the browser
- Theme toggle switches dark/light and persists across reload
- Integration test boots `workdown serve` and asserts `GET /` returns the embedded HTML containing a known marker
- End user installs only the `workdown` binary — no Node, no npm, no pnpm at runtime
- Dev iteration workflow (`pnpm run dev` with proxy) documented and works

## Out of scope

- API endpoints (next slice)
- Real view rendering (next slice)
- Data fetching pattern, JSON envelope, error/warning surface (decided in `first-view-end-to-end` against a real consumer)
- TLS, auth
- Body editing in browser
- Internationalization (English-only for v1; revisit if needed)

## Decisions locked in

- **Svelte 5 (runes).** Explicit reactivity, better TypeScript inference, no Svelte 4 migration cost on a greenfield project.
- **Tailwind via Vite plugin** (build-time). CDN ships the full framework and defeats embedding; the Vite plugin emits only the utilities we use, into the embedded bundle.
- **TypeScript only.** `allowJs: false`, strict config above. Vite/Tailwind/ESLint configs also `.ts`.
- **pnpm** (via corepack, not a global install). Faster, smaller `node_modules`, no network install step in the devcontainer.
- **Dark mode from day one.** Cheap to set up upfront, painful to retrofit across many components. Forces CSS-variable / `dark:` discipline early.
- **Node in the devcontainer, not on the host.** Same pattern as Rust. Host machine stays clean. Build-time-only dependency — end users never touch it.
- **One binary for end users.** `rust-embed` packs the bundle. No runtime Node, no separate static-files directory to ship.

## Open questions

- **DaisyUI on top of Tailwind?** Pre-styled components (buttons, cards, modals) save real time for a developer tool with boards/tables/forms. Costs one dependency, doesn't constrain raw Tailwind usage. Yes/no.
- **Router.** `svelte-spa-router` (~3 KB) vs a 30-line hand-rolled router. SvelteKit is ruled out (assumes its own server).
- **Path-based vs hash-based routing.** Path-based (`/views/board`) needs SPA fallback in axum; hash-based (`/#/views/board`) needs nothing server-side but uglier URLs.
- **Dev workflow ports.** Pick fixed defaults for `workdown serve` and the Vite dev server.
- **Page title, favicon, logo.** Title is "Workdown". Favicon — placeholder SVG, or do we want a real logo first?
- **`cargo build` → Vite trigger.** `build.rs` (rebuild-cache concerns), `xtask`, or a cargo alias. Decide during impl.
- **Folder structure under `ui/src/`.** `components/`, `routes/`, `lib/`, `stores/` — confirm or adjust.

## Followups outside this issue

- `live-updates`'s `depends_on` should change from `[walking-skeleton]` to `[first-view-end-to-end]`: SSE event payloads need the API shape to exist before they can be designed.
