---
id: remaining-read-views
type: issue
status: to_do
title: Remaining read-only views
parent: server
depends_on: [first-view-end-to-end]
---

Add the rest of the view types: table, tree, graph, gantt, plus the chart family (bar, line, workload, metric, treemap, heatmap). Each gets a Svelte component fed by the same `GET /api/views/:id` endpoint; ViewData is type-discriminated, so the endpoint shape is settled. Mostly mechanical work once `first-view-end-to-end` lands.

## Scope

- Per view type, a Svelte component that renders the corresponding `ViewData` variant
- One endpoint handles all types via the discriminated `ViewData` enum
- Graph library decision (Mermaid vs Cytoscape) — pick during impl
- Chart library decision (Chart.js / Observable Plot / d3 / bespoke SVG) — pick before starting the chart family

## Acceptance

- Every view in the test fixture's `views.yaml` renders in the browser
- Components reuse the fetch / error patterns from the first slice

## Open questions

- Card content: what fields show on a card by default? Per-view config or hardcoded for v1?
- Dark mode / theming: in scope here or deferred to polish?
- One chart framework across the family or per-view bespoke?
