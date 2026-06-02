---
id: first-view-end-to-end
type: issue
status: done
title: First view end-to-end (board, read-only)
parent: server
depends_on: [walking-skeleton, ui-foundation]
effort: "2d"
---

Wire one view all the way through the stack. Board is the pick: most visually distinct, lowest risk of looking trivial. This slice is where the API JSON shape, error envelope, warning surface, and Svelte fetch/component patterns get decided — against a real consumer, in one PR.

## Decisions

Backend decisions first, frontend decisions second. Two issues split out during the frontend pass: navigation chrome lives in [[app-shell-navigation]], per-card / per-cell field selection lives in [[view-display-config]] (cross-view-kind).

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

### Frontend: Landing redirect — **first view in `views.yaml`, empty state when none**

`+page.ts` at `/` reads the layout-loaded views list and issues `redirect(307, '/views/${views[0].id}')`. When `views.length === 0`, the root page renders a small empty-state component ("No views configured yet. Add one to `.workdown/views.yaml`."). No HTTP error, no diagnostic — the project is healthy, there's just nothing to render. Users who want a different landing view reorder `views.yaml`.

### Frontend: View page dispatcher — **dedicated `<ViewRenderer>` component**

`+page.svelte` at `/views/[id]` mounts `<DiagnosticBanner>` + `<ViewRenderer data={...} />`. `ViewRenderer` switches on `data.type` and renders the matching view component (`<BoardView>`, future `<TableView>`, etc.). The route page stays focused on page-level orchestration (load + banner + dispatch).

Rejected: inline `{#if data.type === 'board'}` chain inside `+page.svelte`. Same line count, but the diagnostic banner already consumes view data for primary/secondary grouping (two consumers of the same payload at the page level), and a dispatcher component is reusable when item-detail panels later embed view previews — a one-line embed rather than a duplicated if-chain.

### Frontend: Card content — **title + id badge + rendered Markdown body preview**

```
┌─────────────────────────────────┐
│ Implement user login        a1  │   title (or prettified id) + id badge
│                                 │
│ Wire OAuth provider through the │   compact Markdown render of body,
│ existing auth middleware. Need… │   height-capped with mask-gradient fade
└─────────────────────────────────┘
```

Title resolves from `card.title` with fallback to `prettify(card.id)` — always present visually. Id badge in muted monospace, top-right. Body preview is the rendered Markdown (compact mode: collapsed paragraph spacing, tight line-height) inside a `max-height`-capped container with a CSS mask-gradient fade at the bottom for the "more below" visual hint. Empty body → no preview line.

Field-importance heuristics (assignee, deadline, priority, …) are deferred — captured in [[view-display-config]] for cross-view-kind treatment, since the underlying question recurs across boards, tables, trees, graphs, tooltips, item previews.

### Frontend: Markdown rendering — **`marked` + `DOMPurify` + shared `<Markdown>` component**

The library is pulled forward from `mutations-slice` (item-detail panels) into this slice rather than writing throwaway syntax-stripping for card previews. ~50 KB of dependencies (marked ~30 KB, DOMPurify ~20 KB) plus a ~15-line `ui/src/lib/ui/Markdown.svelte` component: parser → sanitizer → `{@html}`. The component supports a `compact` prop that collapses paragraph spacing for card-sized previews.

Rejected: writing a syntax-stripping regex pass for card previews. Non-trivial edge cases (nested syntax, escaped chars, links, images), produces throwaway code, and we'd still need the full library for item-detail surfaces — duplicating effort. Stripping was tempting but undersold the cost.

Mermaid, KaTeX, footnotes, and other GFM-adjacent extensions stay out of scope; if a project's body content uses them, they render as text.

### Frontend: Diagnostic banner — **above `<ViewRenderer>`, grouped by item id, primary/secondary visual split**

Component at `ui/src/lib/ui/DiagnosticBanner.svelte`. Props: `diagnostics: Diagnostic[]` and `viewData: ViewData | undefined` (the banner derives in-view ids itself). Hidden entirely when `diagnostics.length === 0`.

Layout:

```
┌─────────────────────────────────────────────────────────────┐
│ This view                                                   │
│                                                             │
│ task-a                                                      │
│   ✕ field 'parent': broken link to 'epic-1'                 │
│   ✕ field 'depends_on': broken link to 'task-z'             │
│                                                             │
│ task-c                                                      │
│   ⚠ required field 'assignee' is missing                    │
│                                                             │
│ ▸ Other diagnostics (3)                                     │   ← collapsed by default
└─────────────────────────────────────────────────────────────┘
```

When expanded, the secondary section uses synthetic group headers for non-item diagnostics: `Cycles`, `Views`, `Duplicates`.

**Primary/secondary classification rule.** A diagnostic is primary if either (a) at least one of its referenced work item ids is in the current view, OR (b) it's a view-config diagnostic for the *current* view (tier 2 surfacing). Everything else is secondary.

Cycles spanning items in the current view go primary (they affect cards the user is looking at). View-config diagnostics for other views stay secondary.

**Two TS helpers** under `ui/src/lib/diagnostics/`:

- `idsInView(viewData: ViewData) → Set<WorkItemId>` — one match arm per view kind.
- `idsInDiagnostic(diagnostic: Diagnostic) → Set<WorkItemId>` — one match arm per diagnostic kind (ItemDiagnostic → 1 id, Cycle → N ids, DuplicateId → 1 id, ConfigDiagnostic → empty).

**Per-row content:** severity icon (`✕` error, `⚠` warning), item id label (or synthetic group), message from `Diagnostic`'s existing `Display` impl (reused from the CLI — keeps surfaces consistent). Sort: errors before warnings within each item group; item groups by id ascending; synthetic groups last.

**No click behavior in slice 2.** Item-detail navigation lands with `mutations-slice`.

**Semantic color tokens added to `tokens.css`:** `--color-warning-fg`, `--color-warning-bg`, `--color-error-fg`, `--color-error-bg`. First surface that needs them; the rest of the family (success, info) follows whenever a use case lands.

### Frontend: Loading and error states — **blocking `await` in `load()`, errors fall through to `+error.svelte`**

`+page.ts` `await`s `api.getView(id)` — page doesn't render until data arrives. SvelteKit's default navigation indicator handles the visual gap. Cold-load latency is milliseconds on a local project; streaming `{#await}` blocks would add complexity (`{:then}`/`{:catch}` branches, skeleton components) for invisible benefit.

`ui/src/routes/+error.svelte` at the route root handles three error cases by switching on `$page.status`:

- `422` (project unloadable, tier 1 from B5 above) → render the full diagnostic list with the `+error.svelte` template.
- `404` (unknown view id) → "View not found, try one of these:" with links built from the layout-loaded views list.
- Network failure / unexpected throw → generic "Couldn't reach the workdown server" with a refresh hint.

Rejected: streaming `{#await}` (deferred — switch on a per-page basis if latency ever bites).

### Frontend: Empty state — **render empty columns, hide synthetic when empty, subtle hint when board is fully empty**

Each declared column always renders (header visible, body empty) so the board's structure stays legible regardless of card count. The synthetic `value: null` column hides when it has zero cards; when shown, its header reads `(none)` in muted text. When *every* column has zero cards, a single quiet line renders above the columns: "No items to display." No big empty-state graphic, no setup wizard.

Individual empty columns don't carry "no items" text inside — that's noise across many columns on a board with realistic uneven card distributions.

### Frontend: Board visual shape — **flexbox fill-evenly with min-width, horizontal scroll on overflow**

```css
.board   { display: flex; gap: var(--space-4); overflow-x: auto; flex: 1; }
.column  { flex: 1 1 0; min-width: 280px;
           display: flex; flex-direction: column; min-height: 0; }
.cards   { flex: 1; overflow-y: auto; }
```

Behavior:

- Few columns + wide screen → each column gets `(viewport_width − gaps) / N`, fills the available real estate evenly.
- Many columns / narrow screen → columns stay at `280px` minimum, container scrolls horizontally.
- Cards stretch to column width (wider screen → wider cards → more breathing room for the body preview).
- Each column has its own vertical scroll on the cards area; column header sticks to the top of its column.

Rejected: fixed `max-width` per column (wastes screen real estate when only a few columns exist); CSS Grid `repeat(N, minmax(280px, 1fr))` (equivalent behavior, but needs `N` interpolated from JS — flexbox is cleaner here).

Layout CSS is scoped to `BoardView.svelte` / `Column.svelte`. If a second consumer of the same pattern appears in slice 3 (metric tiles is the closest candidate), extract to `lib/ui/Lanes.svelte` or a `.lanes` utility class at that point — premature now with one consumer.

### Frontend: Frontend tests — **deferred**

Manual browser testing for slice 2. Test framework choice (Playwright for E2E vs vitest + Svelte Testing Library for components) wants two or more real components to evaluate against; defer until slice 3 or later when there's more surface to commit against.

## Scope

**Backend (`crates/core` + `crates/server`):**

- `core::load_project()` shared loader returning `Result<Project, LoadError>`; `workdown render` and `workdown validate` refactored to call it.
- `AppState { project_root, config }` constructed by the CLI's `serve` and threaded into the router.
- `GET /api/views` → `ViewSummary[]` flat array inside the envelope's `data`.
- `GET /api/views/:id` → `ViewData` (discriminated union) for any configured view; 404 (empty body) when the id isn't in `views.yaml`.
- Three failure tiers wired per the decisions above: `Err(LoadError)` → 422 with diagnostics; `Ok` with view-config issue for the requested view → 200 + empty data + diagnostics; `Ok` with item-level diagnostics → 200 + data + diagnostics.
- `#[derive(TS)]` on every wire-bound type (full transitive closure listed in the decisions). Generated `.ts` files land in `ui/src/lib/api/generated/` (gitignored).
- Three HTTP integration tests at `crates/server/tests/views_endpoint.rs` against a checked-in fixture project at `crates/server/tests/fixtures/project/`.

**Frontend (`ui/`):**

- `+layout.ts` loads `GET /api/views`; the result is available to every child page (`+page.ts`, `+error.svelte`).
- `+page.ts` at `/` redirects to the first view, or returns empty-state flag when none configured.
- `+page.ts` at `/views/[id]` calls `api.getView(params.id)`, throws SvelteKit `error(...)` on 422/404 so `+error.svelte` catches.
- `+page.svelte` at `/views/[id]` mounts `<DiagnosticBanner>` + `<ViewRenderer>`.
- `lib/views/ViewRenderer.svelte` dispatches on `data.type`.
- `lib/views/board/` — `BoardView.svelte`, `Column.svelte`, `Card.svelte`. Layout per the board-shape decision.
- `lib/ui/Markdown.svelte` — `marked` + `DOMPurify` + `{@html}`, with a `compact` prop for card-sized previews.
- `lib/ui/DiagnosticBanner.svelte` — grouped by item id, primary/secondary classified per the rule.
- `lib/diagnostics/` — `idsInView.ts` and `idsInDiagnostic.ts` helpers.
- `+error.svelte` at the route root — 422 / 404 / network failure handling.
- Semantic color tokens (warning/error pairs) added to `tokens.css`.
- `api.getView(id)` and `api.getViews()` added to `lib/api/client.ts` (the existing `request<T>` helper covers the unwrap).

**Settled internal contracts:** envelope shape, error format and HTTP status codes, `ViewData` serialization with typed field values. All recorded above; code-level annotations (module docs, type comments) carry the rest — no separate ADR.

## Acceptance

- `workdown serve` → browser shows a real board with real data
- Integration tests hit the endpoints against a temp workdown project
- Conventions land as type definitions / module comments, not standalone prose docs

## Out of scope

- Other view types (next slice)
- Mutations (board drag-drop comes with the mutations slice)
- Runtime field selection — defer; revisit when a UI need surfaces
