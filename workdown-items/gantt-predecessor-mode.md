---
id: gantt-predecessor-mode
type: issue
status: to_do
title: Gantt predecessor input mode
parent: renderers
depends_on: [gantt-duration-mode]
---

Extend the Gantt converter with an `after + duration` input mode. The
view data shape does not change — bars always carry resolved
`(start, end)`. The converter resolves predecessors via topological
sort and computes each item's window.

Anchor day rule: same-day. `B.start = A.end` (matches Mermaid's `after`
default). The web app reads `depends_on` directly from the store for
dependency overlay; we don't carry predecessor info on `GanttBar`.

## Pieces

- `views.yaml`: add optional `after` slot to `ViewKind::Gantt`. Points
  to a `links`-typed field (most projects: `depends_on`).
- `views_check`: valid combinations are `(start, end)`,
  `(start, duration)`, or `(after, duration)`. Reject other mixes.
- `views_check`: `after` slot must point to a `links` field with
  `allow_cycles: false` and not an inverse name.
- Converter:
  - Topologically sort filtered items by their `after`-field links.
    Predecessors that are filtered out of the view are still resolved
    against the wider store — dependencies span filters.
  - Per item in topo order: take `max(predecessor.end)` as the anchor;
    `start = anchor`, `end = start + duration_days`.
  - Item with empty `after` field but `after`-mode declared → `unplaced`,
    new reason `NoAnchor`.
  - Item with predecessors that fail to resolve → `unplaced`, new
    reason `PredecessorUnresolved { id }`.
  - Cycles already prevented by the link field's `allow_cycles: false`;
    a cycle reaching the converter indicates a store-validation bug
    (treat as fatal).

## Acceptance

- A view with `after + duration` produces the same `GanttData` shape as
  the other modes.
- Cross-filter predecessor resolution works — a filtered-out predecessor
  still anchors its dependent.
- Snapshot tests for: simple chain, fan-in (multiple predecessors),
  unresolved predecessor, empty `after`, no anchor.

## Out of scope

- Next-day anchor (`B.start = A.end + 1d`) or business-day calendars.
- Per-item mixed mode within one view.
- Visual dependency arrows in Mermaid output — Mermaid Gantt cannot draw
  them. Web app handles this separately as an overlay using the link
  fields directly.
