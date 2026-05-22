---
id: server
type: milestone
status: in_progress
title: Interactive UI (workdown serve)
parent: phase-04-visualization
depends_on: [foundation, item-mutations, renderers]
start_date: 2026-05-07
end_date: 2026-06-25
duration: "7w 1d"
---

End-to-end interactive UI: `workdown serve` boots axum, serves an embedded Svelte SPA, exposes a JSON API the SPA consumes, pushes live updates via SSE. Server and UI ship in one binary and version together — the API is an internal contract between two halves of the same binary, not a public one.

## Architecture (per ADR-006)

- `crates/server/` — axum HTTP + SSE, handlers are thin wrappers over `core`
- `ui/` — Svelte + TypeScript, built by Vite, embedded via `rust-embed`
- Both halves built together; the embedded SPA is the API's only consumer

## Decomposition (vertical slices)

Each slice cuts through the full stack (transport + UI component) and ships a demoable capability. JSON shape, error format, and warning envelope get decided in the first read-view slice, against a real consumer in the same PR. No horizontal "design the API first, then the UI" split — both halves ship together, so they're designed together.

1. `walking-skeleton` — serve boots, embedding pipeline works, browser sees a placeholder Svelte page.
2. `first-view-end-to-end` — one read-only view (board) wired end-to-end. Design vehicle for API and UI patterns.
3. `remaining-read-views` — table, tree, graph, gantt, plus the chart family.
4. `mutations-slice` — drag-drop, field edits, detail panel, create-item form. Save-with-warning surfaced in UI.
5. `live-updates` — file watcher + SSE + browser refetch.

## Out of scope

- TLS, auth — local only
- Remote / multi-user serve
- Public API stability — only consumer is the bundled UI
- Body editing in-browser — users have editors
