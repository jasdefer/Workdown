---
id: view-order-in-extractor
status: to_do
title: Sort and group view items in one place, not per renderer
parent: maintenance-review-2026-08
---

## In plain words

The architecture has a rule: *deciding what a view shows* — including
the order of things — happens once, centrally; the terminal renderer
and the web app only draw the result. Two views break that rule. The
treemap's sort order lives in the terminal-drawing code, and the web
app wrote its own version of the sort — slightly differently, so items
of equal size can already appear in a different order in the terminal
than in the browser. The line chart similarly groups points into
series inside the renderer instead of centrally. Move both decisions
into the central extraction layer so every front end draws the same
pre-decided data.

## The problem in detail

ADR-006 assigns resolution (grouping, ordering, synthetic groups) to
the `view_data` extractors in core; renderers are pure presentation.
Two verified violations, one with observable drift:

**Treemap ordering — drift is real today.**

- `crates/cli/src/render/treemap.rs:99-114` (`sorted_children`) sorts
  children size-descending with an id-ascending tiebreak — in the
  renderer.
- The core extractor (`crates/core/src/view_data/treemap.rs`) only
  sorts `unplaced`, not children.
- The web UI independently re-implements the sort at
  `ui/src/lib/views/treemap/TreemapView.svelte:160` — **without the id
  tiebreak** — so CLI and web can show equal-sized items in different
  orders.

**Line-chart series building.**

- `crates/cli/src/render/line_chart.rs:260-310` (`build_series`)
  partitions points into series, orders them (BTreeMap plus
  synthetic-last), and creates the synthetic `(no <field>)` group —
  all in the renderer. Grouping and the synthetic-group convention are
  extraction-domain (compare: the board's synthetic column and the
  gantt's `(no <field>)` section both come from the extractor). Color
  assignment can stay renderer-side; the partition should not.

**Related small inconsistencies in the extractors themselves** (fix
while in the area):

- `crates/core/src/view_data/treemap.rs:61` and
  `crates/core/src/view_data/workload.rs:163` hand-roll the id-sort
  that `common.rs:303` (`sort_unplaced`) exists for.

## Objective

- Treemap child ordering moves into the core extractor; the CLI
  renderer and `TreemapView.svelte` drop their local sorts and draw in
  received order.
- Line-chart series partitioning and the synthetic-group convention
  move into the core extractor.
- The two hand-rolled unplaced sorts delegate to `sort_unplaced`.

## Out of scope

- Changing what any view shows or how it looks — this only relocates
  the decisions, pinned by the existing snapshot tests.
