---
id: aggregate-rollup
type: issue
status: to_do
title: Compute schema-declared aggregate fields up the parent chain
parent: renderers
---

Wire up the schema's `aggregate:` config so fields with a declared
function are computed automatically on non-leaf items, with values
rolling up the `parent` chain. Today the parser reads
`AggregateConfig`, stores it on `FieldDefinition`, and nothing else
consumes it — so a milestone with descendants holding `start_date`
values still appears blank in views.

CLAUDE.md describes the design: leaf items set values manually,
non-leaves get them computed. Two items in the same ancestor chain
both setting the value manually is a validation error.

## Scope

All variants already declared in `AggregateFunction`:

- numeric (`integer`, `float`): `sum`, `min`, `max`, `average`,
  `median`, `count`
- date: `min`, `max`, `average`
- boolean: `all`, `any`, `none`, `count`

## Acceptance

- A non-leaf item shows the configured aggregate of its descendants'
  leaf values for any field declaring `aggregate:` config.
- `error_on_missing: true` surfaces a diagnostic when a leaf is missing
  the field; `error_on_missing: false` aggregates from available values
  only.
- Validation error when a non-leaf manually sets a field that any
  descendant also sets.
- Computed values are visible to `workdown query`, the table renderer,
  and every other view — indistinguishable from manually-set values.
- Unit tests per aggregate function per compatible field type.

## Out of scope

- Aggregating across non-`parent` link fields (e.g. `depends_on`).
- "Manual override" syntax that lets a non-leaf opt out of aggregation
  with a different value.
