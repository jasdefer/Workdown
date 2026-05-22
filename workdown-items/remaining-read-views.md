---
id: remaining-read-views
type: issue
status: to_do
title: Remaining read-only views
parent: server
depends_on: [first-view-end-to-end]
---

Add the rest of the view types: table, tree, graph, gantt, plus the chart family (bar, line, workload, metric, treemap, heatmap). Each gets a Svelte component fed by the same `GET /api/views/:id` endpoint; ViewData is type-discriminated, so the endpoint shape is settled.

## Status

Table is shipped. Remaining: tree, graph, gantt (+ gantt-by-depth, gantt-by-initiative), chart family (bar, line, workload, metric, treemap, heatmap).

## Scope

- Per view type, a Svelte component that renders the corresponding `ViewData` variant
- One endpoint handles all types via the discriminated `ViewData` enum
- Graph library decision (Mermaid vs Cytoscape) — pick during impl
- Chart library decision (Chart.js / Observable Plot / d3 / bespoke SVG) — pick before starting the chart family

## Acceptance

- Every view in the test fixture's `views.yaml` renders in the browser
- Components reuse the fetch / error patterns from the first slice

## Patterns to reuse (established in the table slice)

- **Title resolution via sidecar `items: HashMap<WorkItemId, ItemRef>`.** When a view's payload references other work items via raw `FieldValue::Link`/`Links` cells, ship a sidecar map keyed by id with titles resolved server-side via the view's `title:` slot. Frontend renders `items[id]?.title ?? prettifyId(id)`; absent ids are broken links. Board/Tree/Gantt already eager-resolve titles inside their per-card structures — sidecar only needed where cells carry raw link values. See `crates/core/src/view_data/table.rs`.

- **Per-column field-type info on the wire.** Anywhere a view ships generic field values via a user-configured column/axis list, include `field_type: FieldType` per slot so the UI renders and aligns deterministically (right-align numbers, format dates, render chips) even when every cell is null.

- **`<Chip>` (`ui/src/lib/ui/Chip.svelte`).** Pill component for set-valued fields (`Choice` / `Multichoice` / `List` / `Links`). Reuse anywhere tag-shaped values appear.

- **Type-driven cell rendering (`ui/src/lib/views/table/Cell.svelte`).** Switches on `FieldType`. Lift to `lib/ui/FieldValueCell.svelte` if a second consumer surfaces (gantt tooltips and tree secondary-field rows are likely candidates).

- **Prettified field labels.** `prettifyId(name)` for any user-configured field name in UI chrome (headers, axis ticks, legends).

- **Sort state — session-local component state.** Persistence (URL / localStorage / `views.yaml`) is deferred to a joint decision with [[view-filter-editor]] and [[view-display-config]]; all three want the same shape.

- **Empty state + count footer copy.** "No items to display." above the view body when empty; "{n} items" / "1 item" muted line below when populated. Uniform across view kinds.

- **Sticky-header + sticky-first-column inside `overflow-x: auto`.** Reusable for any wide-grid-shaped view.

## Known wire-shape gaps to revisit when they bite

- `FieldValue::Duration` serializes as the formatted string (`"5d"`). Frontend sorts/compares lexicographically, which mis-orders mixed magnitudes (`"10d"` < `"2d"`). Easy fix when needed: ship raw seconds alongside the formatted string.

## Open questions

- **Graph library** — Mermaid vs Cytoscape. Decide during the graph slice.
- **Chart library** — pick once before starting the chart family.
