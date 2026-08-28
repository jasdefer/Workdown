---
id: view-order-in-extractor
status: done
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

## Decisions taken

Recorded 2026-08-24 after review. The dividing line agreed on: **core
owns structure and order; each front end owns wording and color.**
ADR-006 mandates that resolution happens in extraction but says nothing
about labels or colors, which is the gap that let four different "no
value" spellings grow. ADR-006 gains one clarifying sentence rather
than a change of rule.

1. **Treemap child ordering moves to core.** Core sorts children
   size-descending, id-ascending on ties, at every level of the tree.
   Both renderers drop their local sorts and draw in received order.
   Rationale: the tie-break needs to know how sizes compare (duration
   vs. number), which is core's knowledge; two independent copies of
   the rule are how the current drift happened.

2. **Line chart: core emits ordered series, not loose points.**
   `LineChartData` carries `series: Vec<LineSeries>`, each with its own
   points — mirroring board's columns-of-cards and gantt's
   sections-of-bars. Rejected alternative: keep flat points plus an
   order hint, which fixes ordering but leaves the partition rule
   duplicated in both renderers. The web's plotting library wants a
   flat array, so it re-flattens and passes the series order along for
   its color domain.

3. **The no-value bucket stays structural in core.** Core reports that
   a series/column/section has no group value; it never ships the
   string `(no team)`. Label text is presentation — it is what you
   would translate, restyle, or render as a heading instead of a
   legend entry.

4. **Each front end gets one shared no-value label helper.** Decision 3
   alone does not fix the drift: core already ships a structural null
   for all three views, and the four spellings exist anyway because
   each renderer converts it to words inline at the draw site. One
   helper per front end, called by board, gantt, and line chart:
   - CLI `render/markdown.rs`: `no_value_label(field)` → `(no team)`
     for inline use, and `no_value_heading(field)` → `No team` for
     board's `##` section heading. Paired in one place so the two
     forms stay deliberate.
   - Web `views/prettify.ts`: `noValueLabel(field)` →
     `(no ${prettifyId(field)})` — the module that already holds the
     other label fallbacks (`cardLabel`, `viewLabel`).

   Accepted scope expansion: this touches board and gantt, which the
   original "out of scope" note excluded. Fixing the line chart's label
   while leaving board's inconsistent would be the wrong trade.

5. **Colors stay renderer-side; their order comes from core.** The
   terminal draws into an SVG with a fixed colorblind-safe palette; the
   browser uses theme-following CSS variables. Genuinely different
   media. What must match is the sequence the palette is walked in,
   which falls out of core deciding series order.

6. **The two hand-rolled unplaced sorts delegate to `sort_unplaced`.**
   No behavior change.

### Accepted asymmetry

The web prettifies every field name it displays (axis labels, legend
titles); the terminal prints them raw. So the web says `(no Team)` and
the terminal `(no team)`. That is a consistent house style within each
front end, not drift — forcing parity would mean changing how one front
end displays field names everywhere. After this work the two agree on
structure, order, and bucket membership, and still differ on
capitalization, consistently.

### Still out of scope

User-configurable sort order in `views.yaml`. A reasonable future
feature — and one that would have to live in core, which is a further
argument for decision 1.
