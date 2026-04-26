---
id: render-gantt-by-depth
type: issue
status: to_do
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

## Pieces

- `views.yaml`: new `type: gantt_by_depth`. Slots: `start`, `end`, plus
  required `depth_link` (a `link` field whose chain defines depth).
- Model: new `ViewKind::GanttByDepth { start, end, depth_link }`.
- `views_check`: `depth_link` must be a `link` field with
  `allow_cycles: false`, not an inverse name.
- Extractor: produces e.g.
  `GanttByDepthData { levels: Vec<Level>, unplaced: Vec<UnplacedCard> }`
  where each `Level { depth: usize, bars: Vec<GanttBar> }`. Depth
  computed by walking `depth_link` upward until no further link.
- Renderer `render_gantt_by_depth(&GanttByDepthData) -> String`:
  - Heading `# Gantt by depth`.
  - Per level: `## Level <n>` then a Mermaid `gantt` block of the same
    shape as basic Gantt.
  - Empty level skipped.
  - Inline unplaced footer at the bottom, same form as basic Gantt.

## Acceptance

- `render_gantt_by_depth(...) -> String` exists and is wired in.
- Output: one chart per depth level with `## Level <n>` subheading.
- Snapshot tests for: shallow tree (only level 0), deep tree, mixed
  depths, empty.
- Renders correctly in GitHub preview.

## Out of scope

- Custom level labels (e.g. `## Milestones` for level 0). Defer until
  the schema can describe per-level labels.
- `duration` and `after` input modes — added once the matching basic
  Gantt followups land.
