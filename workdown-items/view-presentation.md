---
id: view-presentation
type: milestone
status: in_progress
title: View & item presentation
parent: phase-04-visualization
depends_on: [server]
start_date: 2026-07-13
end_date: 2026-07-24
duration: "2w"
---

How views and items *present* their data — a distinct axis from authoring a
view's existence and filter ([[view-authoring]]). Covers customizing which
fields surface on cards, bars, tooltips, and previews, and field types
whose payoff is visual. Builds on the renderers the [[server]] milestone
shipped, which hardcode these presentation choices today.

## Issues

- [[view-display-config]] — per-view-kind display slots: which schema field
  fills each rendered slot (card title/subtitle, bar label, tooltip, …),
  persisted per view with per-user overrides.
- [[color-field-type]] — a `color` field type that tints item surfaces.
  Explicitly paired with display-config: it ships a first-color-field
  convention now and graduates to a real `color:` display slot when
  [[view-display-config]] lands.

## Boundaries

- Not about *which items* a view shows (that's the filter, in
  [[view-authoring]]) or a view's structural shape (its kind and slots).
- Value *formatting* (date/number formats) and computed display fields are
  out of scope — see [[view-display-config]].
