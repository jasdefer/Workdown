---
id: duration-field-type
type: issue
status: to_do
title: Add `duration` field type
parent: renderers
---

Introduce `duration` as the 11th built-in field type, for
quantity-with-time-unit values. Required by `gantt-duration-mode`;
`workload`'s `effort` slot is a likely future migration too.

The 10 existing types (`string`, `choice`, `multichoice`, `integer`,
`float`, `date`, `boolean`, `list`, `link`, `links`) live in
`core::model::schema::FieldType` / `FieldTypeConfig`; this adds one
variant alongside.

## Pieces

- `FieldType::Duration` and `FieldTypeConfig::Duration { unit }` in
  `core::model::schema`.
- Unit enum: `Days`, `Weeks`, `Hours`. Per-field declared in
  `schema.yaml` as `unit: days | weeks | hours`. Internal storage TBD
  during implementation (probably a single canonical unit such as
  minutes or seconds, with the configured display unit retained for
  formatting).
- Parser: accept either bare integer (interpreted in the field's unit)
  or unit-suffixed string (`5d`, `2w`, `4h`). Reject mismatched suffix
  (e.g. `5h` on a `unit: days` field) — clear error.
- `FieldValue::Duration(...)` carrying enough to format and compare
  (exact shape: implementation-time choice).
- `core::query::format::format_field_value`: render durations with unit
  suffix, e.g. `5d`, `2w`, `4h`.
- Table renderer cell formatting: same form as `format_field_value`.
- `defaults/schema.schema.json`: formal definition of the new type.
- `defaults/schema.yaml`: leave alone (no project-default duration
  field).
- Validation: non-negative; integer-only for v1 (no `1.5d` etc.).

## Acceptance

- `schema.yaml` accepts `type: duration, unit: days|weeks|hours`.
- Round-trips through parser → store → render.
- Existing 10 field types unaffected (regression-tested).
- Unit tests for parse, format, validate (per unit and per malformed
  input).
- `workdown query` and the table renderer display durations correctly.

## Out of scope

- Migrating `workload`'s `effort` slot to `duration`: that's a separate
  issue tied to `render-workload`.
- Sub-day units below hours (minutes/seconds).
- Fractional durations.
- Mixed-unit arithmetic (e.g. adding hours to days). Comparison and
  conversion within the same canonical unit only.
