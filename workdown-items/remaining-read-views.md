---
id: remaining-read-views
type: issue
status: in_progress
title: Remaining read-only views
parent: server
depends_on: [first-view-end-to-end]
---

Add the rest of the view types: table, tree, graph, gantt, plus the chart family (bar, line, workload, metric, treemap, heatmap). Each gets a Svelte component fed by the same `GET /api/views/:id` endpoint; ViewData is type-discriminated, so the endpoint shape is settled.

## Status

Board, table, tree, and graph are shipped. Remaining: gantt (+ gantt-by-depth, gantt-by-initiative), chart family (bar, line, workload, metric, treemap, heatmap).

The graph slice settled the graph-library question: **Cytoscape** with the **cytoscape-dagre** layout — the same layered-DAG algorithm Mermaid's `flowchart TD` uses. The built-in breadthfirst/cose layouts ordered nodes noticeably worse and were dropped. Cytoscape is dynamically imported so it only loads on graph pages; node hover reuses the board `<Card>` for a title/id/body popover; click-to-open is deferred with the other views to the item-page slice.

## Scope

- Per view type, a Svelte component that renders the corresponding `ViewData` variant
- One endpoint handles all types via the discriminated `ViewData` enum
- Graph library decision (Mermaid vs Cytoscape) — pick during impl
- Chart library decision (Chart.js / Observable Plot / d3 / bespoke SVG) — pick before starting the chart family

## Acceptance

- Every view in the test fixture's `views.yaml` renders in the browser
- Components reuse the fetch / error patterns from the first slice

## Patterns to reuse (established in the table and tree slices)

- **Title resolution via sidecar `items: HashMap<WorkItemId, ItemRef>`.** When a view's payload references other work items via raw `FieldValue::Link`/`Links` cells, ship a sidecar map keyed by id with titles resolved server-side via the view's `title:` slot. Frontend renders `items[id]?.title ?? prettifyId(id)`; absent ids are broken links. Board/Tree/Gantt already eager-resolve titles inside their per-card structures — sidecar only needed where cells carry raw link values. See `crates/core/src/view_data/table.rs`.

- **Per-column field-type info on the wire.** Anywhere a view ships generic field values via a user-configured column/axis list, include `field_type: FieldType` per slot so the UI renders and aligns deterministically (right-align numbers, format dates, render chips) even when every cell is null.

- **Shared `Column { name, field_type }` in `view_data::common`.** Lifted from `view_data/table.rs` when the tree slice needed identical column metadata. `build_column(name, schema)` and `column_cell(name, item)` are the helpers — every future column-bearing view should reuse them rather than reimplementing virtual-`id` resolution.

- **`<Chip>` (`ui/src/lib/ui/Chip.svelte`).** Pill component for set-valued fields (`Choice` / `Multichoice` / `List` / `Links`). Reuse anywhere tag-shaped values appear.

- **Type-driven cell rendering (`ui/src/lib/views/table/Cell.svelte`).** Switches on `FieldType`. Tree reuses it directly via `$lib/views/table/Cell.svelte` import; consider lifting to `lib/ui/FieldValueCell.svelte` only if a third consumer surfaces or if circular import concerns arise.

- **`ColumnResizeHandle` (`ui/src/lib/views/ColumnResizeHandle.svelte`).** Generic right-edge drag handle. Pointer Events + `setPointerCapture`, writes to a `SvelteMap<number, number>` keyed by column index, optional `onBeforeStart` for views that need to seed sibling widths (table uses it to engage `table-layout: fixed` cleanly). Skip the last column.

- **CSS Grid "outline grid" pattern.** Distinct from the table's semantic `<table>` — used for views that need hierarchy + columns in one layout (tree, and a candidate for future variants). Rows wrapped in `<div role="row" style="display: contents">` so each cell becomes a direct grid child for column alignment across depth levels. Sticky first column via `position: sticky; left: 0`.

- **All-collapsed-on-load disclosure state.** `SvelteSet<string>` at the view's top, passed down via prop to recursive children. No persistence; same bucket as sort/resize, all join `view-display-config` later. Recursive components call `<svelte:self>` for visible children inside `{#if expanded}`.

- **Prettified field labels.** `prettifyId(name)` for any user-configured field name in UI chrome (headers, axis ticks, legends).

- **Sort / expansion / resize state — session-local component state.** Persistence (URL / localStorage / `views.yaml`) is deferred to a joint decision with [[view-filter-editor]] and [[view-display-config]]; all three want the same shape.

- **Empty state + count footer copy.** "No items to display." above the view body when empty; "{n} items" / "1 item" muted line below when populated. Uniform across view kinds.

- **Sticky-header + sticky-first-column inside `overflow-x: auto`.** Reusable for any wide-grid-shaped view.

- **Canvas-renderer theming (established in the graph slice).** Renderers that draw to `<canvas>` (Cytoscape, and the upcoming chart libraries) can't inherit `var(--color-*)`. Read resolved token values via `getComputedStyle(document.documentElement).getPropertyValue(...)` at build time and re-apply on theme flip — an `$effect` that reads `themeStore.value`. Also dynamically `import()` heavy renderer libs so they only load on pages that use them. See `ui/src/lib/views/graph/GraphView.svelte`.

- **Inline secondary fields in markdown trees.** `- [Title](path) — name: value · name: value` with em-dash separator and middle-dot join. Em dash and entire suffix suppressed when no cells are set. Worth replicating for any future markdown renderer that wants to surface secondary fields beside link text.

## Consistency notes for upcoming view implementations

When adding the next view (graph / gantt / chart family), keep these in lockstep with the shipped slices:

- **Backend wire types are declared by appending to `crates/core/examples/gen_types.rs`** — both the `ALL_TYPES` list and a `write_type::<…>` call. Forgetting either yields broken TS imports. Run `cargo xtask gen-types` to regenerate `ui/src/lib/api/generated/*.ts`.

- **`ts_rs::TS` derive on every wire type.** Match the shape of existing `view_data` structs: `#[derive(Debug, Clone, Serialize, ts_rs::TS)]`. Carry `ts(type = "...")` overrides only when serde's runtime serializer produces something other than the Rust type's natural shape (see `FieldValue::Duration`).

- **Add the new branch to `ui/src/lib/views/ViewRenderer.svelte`.** The discriminator is `data.type` — string-snake-case from the Rust enum (`gantt_by_depth`, `bar_chart`, etc.). Forgetting this branch shows the placeholder "View kind X is not yet rendered" message.

- **One-line view description in `crates/cli/src/render/description.rs`.** Even the markdown-only renderers (graph, gantt) need a caption; the dispatcher feeds it to the view-specific renderer.

- **Validation in `crates/core/src/views_check.rs`.** Every view kind has its own branch in `check_view` — type-check every slot, cross-check mutually-exclusive slots, validate `where:` clauses. Mirror the existing patterns (`check_slot`, `check_link_slot`, `check_aggregate_value_slot`).

- **Schema and defaults.** When introducing a new slot, update `crates/core/defaults/views.schema.json` (for editor autocomplete) and add an exercising entry to `.workdown/views.yaml` and `crates/server/tests/fixtures/project/.workdown/views.yaml`. The schema is shipped to consumer projects; defaults are the dev/test fixture.

- **CLAUDE.md memory rule: only `id` is privileged.** Don't hardcode field names like `status` / `parent` / `title` anywhere — every view kind takes its driving field via its `ViewKind::*` variant slot, validated against `schema.yaml`. The tree view's `field:` slot is "the link field the user picked," not "parent."

- **Devcontainer build flow.** `cargo` doesn't auto-build the UI. For testing in the browser:
  - Production-like: `npm --prefix ui run format && cargo xtask build-ui && cargo run -- serve` → `:3141`
  - Hot reload: leave `cargo run -- serve` running, then `npm --prefix ui run dev -- --host` → `:5173` (the `--host` flag is mandatory for the devcontainer port forwarding to expose Vite)
  - HMR can lose sync after big file restructures — restart `npm run dev` if Svelte changes stop showing up

- **Commit style.** Read-only view slices have shipped under `Land <view-name>: <one-line capability>` (see `7652667` board, `c41b1db` table, `e5badb7` tree). Match that for the graph / gantt / chart commits.

## Known wire-shape gaps to revisit when they bite

- `FieldValue::Duration` serializes as the formatted string (`"5d"`). Frontend sorts/compares lexicographically, which mis-orders mixed magnitudes (`"10d"` < `"2d"`). Easy fix when needed: ship raw seconds alongside the formatted string.

## Open questions

- **Chart library** — pick once before starting the chart family.
