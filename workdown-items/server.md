---
id: server
type: milestone
status: to_do
title: Interactive server
parent: phase-04-visualization
depends_on: [foundation, renderers, item-mutations]
---

Local web server powering the interactive UI. Lives in `crates/server/`, invoked via `workdown serve`. Thin HTTP layer over `core` — every handler calls core functions directly, no shell-out.

## Goals

- `workdown serve` starts axum, serves the embedded frontend, exposes a JSON API
- Query endpoints (list views, get view data, get item data) and mutation endpoints (set field, create item) — all calling into `core`
- Runtime field selection: `/board?field=priority` renders against whichever compatible field the user picks
- File watching + Server-Sent Events so the browser auto-refreshes when items change on disk

## Notes

Issues here are sketched at a coarse level. Decompose further when we know more from implementing `foundation` and `renderers`.
