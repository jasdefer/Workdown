---
id: first-view-end-to-end
type: issue
status: to_do
title: First view end-to-end (board, read-only)
parent: server
depends_on: [walking-skeleton, ui-foundation]
---

Wire one view all the way through the stack. Board is the pick: most visually distinct, lowest risk of looking trivial. This slice is where the API JSON shape, error envelope, warning surface, and Svelte fetch/component patterns get decided — against a real consumer, in one PR.

## Decisions

Backend decisions discussed in turn and recorded here. Frontend decisions follow in a second pass — `### Frontend …` headings will be appended as they're settled.

### Project loading strategy — **cold-load per request, no cache**

Every HTTP request re-parses the project from disk via `core::load_project()` (see next decision). Parsing hundreds of work items takes milliseconds — well below human-perceptible latency, and no cache means no invalidation logic to keep in sync with the future watcher.

Rejected: cache at startup (forces restart on every Markdown edit, hostile UX); cache + invalidate via watcher (couples two things `live-updates` keeps orthogonal — invalidation and SSE push). With cold-load, `live-updates`'s watcher has exactly one job: push SSE events to the browser. No server-side cache to evict.

### Project loader — **shared `core::load_project()` consumed by server + CLI**

A new module in `crates/core` exposes `fn load_project(config, project_root) -> Result<Project, LoadError>` where `Project` carries `{ store, schema, views, calendar, diagnostics }`. The view handler becomes ~5 lines: load → extract → wrap in `ApiResponse`. `workdown render` and `workdown validate` get refactored to call the same function — they currently each assemble the same orchestration inline.

Rejected: server inlines its own load sequence. Three call sites diverge over time; when slice 5's SSE handler joins, it's four. One name with one definition is cheaper to maintain.

### Server state — **paths only: `AppState { project_root, config }`**

The CLI's `serve` command parses `Config` at startup (already happens today for port resolution) and threads `(project_root, config)` into the server. Every handler does its own load. No preloaded schema, no `OnceCell`, no mutex — keeps the server stateless beyond "where the project lives." Falls out trivially from the cold-load decision; no real alternatives.

### Diagnostics on the wire — **flat array, UI groups for display**

Every response carries `diagnostics: Diagnostic[]` as a flat list — no server-side filtering or pre-grouping. The full project's diagnostics (broken links, cycles, view-config issues, item validation findings) ride along on every view response.

The UI groups in one shared `DiagnosticBanner` component that the view-page dispatcher mounts above whatever view renders below. A small TS helper `idsInView(viewData: ViewData) -> Set<WorkItemId>` (one match arm per view kind) tells the banner which diagnostics reference items inside the current view (primary, highlighted) vs outside (secondary, greyed). Individual view components never think about diagnostics.

Rejected: server returns `{ primary, secondary }` (wire shape diverges from "always just `diagnostics: Diagnostic[]`"; future non-view endpoints would have to invent their own grouping vocabulary); return only diagnostics touching the current view (hides global problems the user should know about); return only `views_check` diagnostics (broken links never surface until `/api/diagnostics` ships, which is post-milestone).

### Failure tiers — **three categories, each with a distinct UI surface**

| Tier | Example | HTTP | UI |
|---|---|---|---|
| Project unloadable | `schema.yaml` missing, `views.yaml` unparseable | `422` + diagnostics, no `data` | `+page.ts` calls SvelteKit's `error()`; `+error.svelte` renders a full error page with the diagnostic list |
| This view misconfigured | Gantt missing `duration`, board references unknown field | `200 OK` + empty `data` + diagnostics | View page renders; banner explains; main area shows "this view can't render" empty state |
| Items have issues | Broken parent link, validation warning | `200 OK` + full `data` + diagnostics | Banner shows primary/secondary grouping per the rule above |

Internally `load_project()` returns `Result<Project, LoadError>`. `Err` → tier 1 (handler maps to 422). `Ok` with `views_check` diagnostics referencing this view → tier 2. `Ok` with other diagnostics → tier 3.

### Unknown view ID — **404 with empty body**

`GET /api/views/typo` → `404`, no body. The UI's `+layout.ts` already loads the full views list for navigation, so the `+error.svelte` page for 404 can show "View `typo` is not configured. Try: status-board, hierarchy, …" entirely client-side, without a server-side diagnostic.

Rejected: 404 with a synthesized `Diagnostic`. "View not found" isn't a validation finding — no source path, no item, no view-config issue. A diagnostic for "the URL is wrong" would dilute the vocabulary.

### `GET /api/views` shape — **`ViewSummary[]` flat array**

```rust
#[derive(Serialize, TS)]
pub struct ViewSummary {
    pub id: String,
    pub title: Option<String>,
    pub kind: ViewKindLabel,
}

#[derive(Serialize, TS)]
#[serde(rename_all = "snake_case")]
pub enum ViewKindLabel {
    Board, Tree, Graph, Gantt, GanttByDepth, GanttByInitiative,
    Table, Heatmap, Treemap, BarChart, LineChart, Metric, Workload,
}
```

The endpoint returns the array directly inside the envelope's `data` field. The landing page at `/` redirects to the first view in `views.yaml`, or renders an empty state when the array is empty. Users who want a different landing view reorder `views.yaml` — file order doubling as priority is fine for something this lightweight, and visibly the first entry comes first. `title` stays optional; the UI prettifies the id when it's unset.

Rejected: full `View` shape (leaks per-kind config the navigation doesn't need, ties the wire response to internal model evolution); `{ views, default: Option<String> }` wrapper with a `defaults.default_view:` config field (extra noise in `config.yaml` for a behavior file-order already expresses); separate `/api/default-view` endpoint (two round trips for what's now zero — the UI uses `views[0]`).

### ts-rs scope — **all wire-bound types now, generated per-file**

`#[derive(TS)]` lands on the full transitive closure of types that touch the wire:

- `ViewData` + every variant (`BoardData`, `GanttData`, `TreeData`, `TableData`, `GraphData`, `HeatmapData`, `TreemapData`, `LineChartData`, `BarChartData`, `MetricData`, `WorkloadData`, `GanttByDepthData`, `GanttByInitiativeData`)
- View-internal types (`Card`, `CardField`, `BoardColumn`, `GanttBar`, `TreeNode`, `Edge`, …)
- `FieldValue` and every variant, `WorkItemId`, `UnplacedCard`, `UnplacedReason`, `AggregateValue`, `AxisValue`, `SizeValue`
- Full `Diagnostic` chain (`Severity`, `DiagnosticBody`, all four body variants and their `Kind` enums)
- New `ViewSummary`, `ViewKindLabel`

Slice 2 only renders the board, but every type is decorated so slices 3-5 are pure Svelte work — no Rust changes for ts-rs scope expansion. Generated files land in `ui/src/lib/api/generated/` (gitignored, per `ui-foundation`); ts-rs's default per-file output stays as-is.

### HTTP integration tests — **three minimal tests against a checked-in fixture**

Tests in `crates/server/tests/views_endpoint.rs` exercise the contract:

1. `GET /api/views` returns the fixture's configured views as `ViewSummary[]`.
2. `GET /api/views/<board-id>` returns a `BoardData` payload with expected columns.
3. `GET /api/views/typo` returns `404`.

Run via `tower::ServiceExt::oneshot` against `workdown_server::router(state)` — no real server, no browser. Fixture at `crates/server/tests/fixtures/project/` (sibling of `fixtures/dist/`) with a minimal `.workdown/` + a handful of items.

Edge cases (unparseable views.yaml, missing schema, broken links) are tested at the `core::load_project` layer, not at the HTTP layer. UI testing stays manual for slice 2; framework choice deferred until there's more component surface to test against.

## Scope

- `GET /api/views` — list configured views from `views.yaml`
- `GET /api/views/:id` — return `ViewData` as JSON for a board view
- Svelte board component renders columns and cards from the JSON
- Fetch layer + error/loading state pattern established for later slices
- API conventions decided here (code review is sufficient — no separate ADR; internal contract):
  - JSON envelope shape
  - Error format and HTTP status codes
  - How `ViewData` serializes (especially typed field values)

## Acceptance

- `workdown serve` → browser shows a real board with real data
- Integration tests hit the endpoints against a temp workdown project
- Conventions land as type definitions / module comments, not standalone prose docs

## Out of scope

- Other view types (next slice)
- Mutations (board drag-drop comes with the mutations slice)
- Runtime field selection — defer; revisit when a UI need surfaces
