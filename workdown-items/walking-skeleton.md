---
id: walking-skeleton
type: issue
status: to_do
title: workdown serve skeleton with embedded UI
parent: server
---

End-to-end smoke test of the build and serve pipeline: `workdown serve` boots axum, the Vite-built Svelte bundle is embedded via `rust-embed`, the browser loads a placeholder page. No real API, no real UI — just proves the plumbing all the way from `cargo build` to a working page in the browser.

## Scope

- `workdown serve [--port N] [--open]` CLI command (defaults: auto-port, no browser open)
- `ui/` Svelte + TypeScript project initialized, Vite builds to `ui/dist/`
- `cargo build` invokes the Vite build (build.rs, xtask, or workspace alias — decide during impl)
- `rust-embed` embeds `ui/dist/` into the server crate
- `GET /` serves embedded `index.html`; `GET /assets/*` serves the bundle
- Startup log line: URL, port, PID

## Acceptance

- Fresh clone → `cargo build --workspace` → `workdown serve` serves the Svelte placeholder
- No node required at runtime; node only needed at build time
- Dev workflow documented (`npm run dev` during UI iteration, `cargo build` for embedding)

## Out of scope

- API endpoints
- TLS, auth

## Open questions

- Does `build.rs` invoking `npm run build` cause rebuild-cache pain? If so, fall back to a cargo alias or xtask-style wrapper.
