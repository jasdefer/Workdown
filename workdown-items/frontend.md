---
id: frontend
type: milestone
status: to_do
title: Frontend
parent: phase-04-visualization
depends_on: [server]
start_date: 2026-05-27
end_date: 2026-06-25
duration: "4w 2d"
---

Svelte + TypeScript UI, built with Vite, embedded into the workdown binary via `rust-embed`. Runs in the browser when `workdown serve` is active.

**Decomposition deferred** until the `server` milestone lands — the frontend contract depends on what the API looks like in reality.

## Known scope (for context, not as issues yet)

- Board view with drag-drop → `POST /api/items/:id/field` (only view type with mutations in v1)
- Tree view (click to expand, read-only in v1)
- Graph view (Mermaid v1; Cytoscape if interactive graph editing becomes a priority)
- Table view (sortable columns; inline editing of selected fields — decide per-field during impl)
- Gantt view (read-only; Mermaid-rendered)
- Bar chart, line chart, workload, treemap, heatmap — read-only static renders inside the app
- Metric cards — single-number displays (composable into a future dashboard view, deferred)
- Runtime field selection for every view type: user picks any compatible field at view time
- Item detail panel with field editing
- Read-only rendering of the markdown body inside the detail panel (body edits happen in the user's editor)
- Create-item form
- SSE subscription for auto-update
- Validation-warning display when save-with-warning returns warnings

## Known open questions

- Graph library for the live view: Mermaid vs Cytoscape. Decide when we get here.
- Card content: what fields show on a card by default? Per-view config?
- Dark mode / theming: in scope for v1 or defer?
- Chart library strategy: one framework (Chart.js / Observable Plot / d3) or per-view bespoke SVG? Decide before starting chart renderers.
