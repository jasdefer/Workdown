---
id: ui-build-integration
type: issue
status: to_do
title: UI build integration and asset embedding
parent: server
depends_on: [serve-command-skeleton]
---

Wire the `ui/` Svelte + TypeScript build into `cargo build` and serve the compiled assets from the server via `rust-embed`. Result: one `cargo build` produces a complete binary including the frontend.

## Scope

- Vite build produces `ui/dist/` (HTML, JS, CSS)
- `cargo build` invokes the Vite build (via `build.rs` or a workspace script — decide during impl)
- `rust-embed` embeds `ui/dist/` into the server crate
- `GET /` serves the embedded `index.html`
- `GET /assets/*` serves the embedded bundle
- The binary runs without node present at runtime — node is only needed for the initial build

## Acceptance

- Fresh clone → `cargo build --workspace` → `workdown serve` serves a real HTML page (even just Svelte's "hello world")
- Development workflow documented (e.g. `npm run dev` during UI iteration, `cargo build` when embedding)

## Open questions

- Does `build.rs` invoking `npm run build` cause rebuild-cache pain? If yes, fall back to a cargo alias or `xtask`-style wrapper.
