---
id: gantt-duration-mode
type: issue
status: done
title: Gantt duration input mode
parent: renderers
depends_on: [render-gantt, duration-field-type]
---

Extend the Gantt converter with a `start + duration` input mode. The
view data shape (`GanttData`) does not change — bars always carry
resolved `(start, end)`. The converter reads the configured `duration`
field, converts to days using the field's declared unit, and computes
`end = start + days`.

## Pieces

- `views.yaml`: add optional `duration` slot to `ViewKind::Gantt`
  alongside the existing `start`, `end`.
- `views_check`: enforce a valid combination per view. `start` is
  required; exactly one of `end` or `duration` must be set, not both,
  not neither. Clear error messages.
- `views_check`: when `duration` is set, the slot must point to a
  `duration`-typed field.
- Converter (`view_data::gantt::extract_gantt`): when `duration` is
  configured, look up the value, convert to days via the field's unit,
  compute `end = start + days`. Item missing the duration value goes to
  `unplaced` with `MissingValue { field }`.
- Bar shape unchanged — Mermaid output identical for equivalent inputs.

## Acceptance

- A view with `start + duration` produces equivalent `GanttData` to one
  with `start + end` for the same intent.
- Mermaid output identical between equivalent inputs.
- Snapshot tests for: integer-days field, week-unit field, hour-unit
  field, missing duration value.

## Out of scope

- `gantt-predecessor-mode`: separate issue.
- Per-item mixed mode (one bar uses `end`, another uses `duration`
  within the same view).
- Sub-day rounding rules — assume duration values are exact at
  conversion time; rounding rules added if a real case demands them.
