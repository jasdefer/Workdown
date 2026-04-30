---
id: server
type: milestone
status: to_do
title: Interactive server
parent: phase-04-visualization
depends_on: [foundation, item-mutations]
start_date: 2026-05-07
end_date: 2026-05-26
duration: "20d"
---

Local web server powering the interactive UI. Lives in `crates/server/`, invoked via `workdown serve`. Thin HTTP layer over `core` — every handler calls core functions directly, no shell-out.

## Goals

- `workdown serve` starts axum, serves the embedded frontend, exposes a JSON API
- Query endpoints (list views, get view data for any v1 type — board, tree, graph, table, gantt, bar_chart, line_chart, workload, metric, treemap, heatmap — get item data) and mutation endpoints (set field, create item) — all calling into `core`
- Runtime field selection: `/view?type=board&field=priority` renders against whichever compatible field the user picks; same pattern for the other view types with their own slot names
- File watching + Server-Sent Events so the browser auto-refreshes when items change on disk

## Notes

Issues here are sketched at a coarse level. Decompose further when we know more from implementing `foundation` and `renderers`.
