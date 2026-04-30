---
id: render-gantt-by-depth
type: issue
status: done
title: Gantt by depth view
parent: renderers
depends_on: [render-gantt]
---

New view type `gantt_by_depth` (option E from the Gantt design
discussion). Emits one Mermaid Gantt block per depth level of a link
chain — level 0 = roots, level 1 = their direct children, etc. Useful
for "rollup" reading: top-level milestones in one chart, epics in the
next, stories below.

Caveat to set expectations: tree depth in real projects is asymmetric.
Different branches reach different depths, and items at the same numeric
depth aren't necessarily semantically equivalent (some have children,
some don't). The rendered charts reflect this honestly. If you want
"milestones vs. epics vs. stories" use `where:` filters with `type:`
instead.

Everything below is a starting point — challenge any of it.

## Suggested shape

- `views.yaml`: new `type: gantt_by_depth`. Likely slots: `start`, `end`,
  `duration`, `after` (mirroring basic Gantt — all three input modes
  already exist), plus required `depth_link` (a `link` field whose chain
  defines depth).
- Model: new `ViewKind::GanttByDepth { start, end, duration, after, depth_link }`.
- Extractor: `GanttByDepthData { levels: Vec<Level>, unplaced: Vec<UnplacedCard> }`
  where `Level { depth: usize, bars: Vec<GanttBar> }`. Depth computed by
  walking `depth_link` upward against the full store until no further
  link.
- Renderer `render_gantt_by_depth(...)`:
  - Heading `# Gantt by depth`.
  - Per non-empty level: `## Level <n>` + Mermaid `gantt` block.
  - Inline unplaced footer at the bottom.

## Shared infrastructure already in place (from `gantt-by-initiative`)

Lean on these — don't reimplement:

- `view_data::gantt::resolve_bars(view, store, schema, &GanttResolution)` —
  handles all three input modes, returns bars + sorted unplaced.
- `render::gantt_common::{render_gantt_block, render_unplaced_footer, label_for, sanitize_label}` —
  the inner Mermaid block builder, label sanitizer, and footer.
- `views_check::check_gantt_input_modes` — already lifted, both existing
  Gantt variants call it; this one would too.
- `views_check::check_root_link_slot` — template for `check_depth_link_slot`
  (Link, `allow_cycles: false`, not an inverse name). Likely just two
  new diagnostic kinds (`ViewGanttDepthLinkCyclic`,
  `ViewGanttDepthLinkInverseNotAllowed`) — or generalize the link-slot
  diagnostics across `after` / `root_link` / `depth_link` (out of scope
  here unless cheap).

## Decisions inherited from `gantt-by-initiative` (worth challenging if you disagree)

- Support all three input modes from day 1 (`start+end`,
  `start+duration`, `start+after+duration`).
- No internal `group` slot per chart — chart is already scoped to one
  level.
- JSON schema kept loose; `views_check` enforces input-mode combos with
  structured diagnostics.
- Cycle defense via visited set returns the current item as effective
  root; no `Cycle` unplaced reason needed (the bar is still renderable
  even if the partition is ambiguous).
- Walk `depth_link` against the full store, not the filtered set, so
  chains span filter boundaries.

## Specific to depth

- `walk_to_depth(item, depth_link, store) -> usize` — counts steps from
  `item` up to a root, with a visited set. Mirrors `walk_to_root`'s
  structure but returns the count.
- Level 0 = items whose `depth_link` value is absent or points at a
  non-existent id (broken link → effective root → depth 0).
- Levels in output sorted ascending; bars within a level by `(start, id)`.

## Acceptance

- `render_gantt_by_depth(...)` exists and is wired into `workdown render`.
- Output: one chart per non-empty depth level with `## Level <n>`
  subheading.
- Snapshot tests covering: shallow tree (only level 0), deep tree, mixed
  depths, empty.
- Renders correctly in GitHub preview.

## Out of scope

- Custom level labels (e.g. `## Milestones` for level 0). Defer until
  the schema can describe per-level labels.

## Worth discussing before coding

- **Filtered-out intermediate ancestors**: if `leaf` has `parent: mid`
  and `mid` has `parent: root`, but `mid` is filtered out, what depth
  does `leaf` show at? Walking the full store gives depth 2. Walking the
  filtered set gives depth 1. By-initiative chose full-store walk for
  partitioning — same answer probably right here, but worth confirming.
- **Per-level ordering**: alphabetical sort of root ids made sense for
  by-initiative because each chart was an entity. Levels are numbered;
  ascending depth is the obvious default. Confirm.
