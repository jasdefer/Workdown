---
id: render-gantt-by-initiative
type: issue
status: done
title: Gantt by initiative view
parent: renderers
depends_on: [render-gantt]
---

New view type `gantt_by_initiative` (option F from the Gantt design
discussion). Emits one Mermaid Gantt block per top-level ancestor of a
configurable link chain — each chart shows the items belonging to one
initiative. Sidesteps Mermaid's flat-section limitation by using
multiple charts instead of trying to nest sections.

## Pieces

- `views.yaml`: new `type: gantt_by_initiative`. Slots: `start`, `end`
  (and later `duration`/`after` mirroring basic Gantt as those modes
  land), plus required `root_link` (a `link`-typed field whose chain we
  walk to find the root, e.g. `parent`).
- Model: new `ViewKind::GanttByInitiative { start, end, root_link }`
  variant in `core::model::views`. Same parser/JSON-schema treatment as
  the existing variants.
- `views_check`: `root_link` must be a `link` field with
  `allow_cycles: false`, not an inverse name. `start`/`end` validated
  the same way as basic Gantt.
- Extractor: produces e.g.
  `GanttByInitiativeData { initiatives: Vec<Initiative>, unplaced: Vec<UnplacedCard> }`
  where each `Initiative { root: Card, bars: Vec<GanttBar> }`. Items
  partitioned by walking `root_link` to root; items with no link in the
  chain (root themselves) live in their own initiative. Initiative
  ordering: alphabetical by root id.
- Renderer
  `render_gantt_by_initiative(&GanttByInitiativeData) -> String`:
  - Heading `# Gantt by initiative`.
  - Per initiative: `## <root title>` then a Mermaid `gantt` block of
    the same shape as basic Gantt, no internal sections (the chart is
    already scoped to one initiative).
  - Empty initiative skipped (no chart, no heading).
  - Inline unplaced footer at the bottom of the document, same form as
    basic Gantt.
- `commands/render.rs`: add dispatch arm; orchestrator emits unplaced
  warnings the same way.

## Acceptance

- `render_gantt_by_initiative(...) -> String` exists and is wired into
  `workdown render`.
- Output: one chart per top-level ancestor with `##` subheading.
- Snapshot tests for: single initiative, multiple initiatives, all-orphan
  (no chains), empty.
- Renders correctly in GitHub preview.

## Out of scope

- Per-initiative `where`-filter overrides (use the view-level `where`).
- Custom initiative ordering — alphabetical by root id for v1.
- `duration` and `after` input modes — added once the matching basic
  Gantt followups land.
