---
id: computed-fields
type: issue
status: in_progress
title: Computed fields — same-item cross-field expressions
parent: time-tracking
depends_on: [project-constants]
effort: "8h"
---

A field can declare a `compute:` expression over other fields of the
same item: `end_date` from `start_date + duration`, `cost` from
`effort * $constants.daily_rate`, `flow_efficiency` from
`effort / duration`. This is the horizontal counterpart to aggregation
(cross-item, same field): same item, cross-field. Together the two
compose — `cost` computed per leaf, then summed up the parent chain —
which directly serves this milestone's "derived measurements" theme
(flow efficiency, lead time, cycle time).

Computed values are never stored. Like aggregates (`store/rollup.rs`),
they are resolved into the in-memory model at load time and are
indistinguishable from manually-set values for `workdown query` and
every view. Frontmatter stays human input only.

## Scope

- `compute:` config on a field definition, holding an expression over
  other field names and `$constants.<name>` references
  ([[project-constants]]).
- Minimal expression syntax: `+ - * /` and parentheses. No
  conditionals, no functions.
- Closed type algebra, checked against the schema at load time:
  `date ± duration → date`, `date − date → duration`,
  `duration ± duration → duration`, `duration / duration → float`,
  numeric arithmetic on `integer`/`float` (division → float),
  `number * duration → duration`. The expression's result type must
  match the field's declared type — anything else is a schema error.
- Evaluation order: dependency graph across computed and aggregated
  fields (compute leaves → aggregate up → compute fields on non-leaves
  that consume aggregated inputs). Cycles among field expressions are
  a schema-load error.
- One direction per field: `end_date: compute: start_date + duration`
  is a declaration, not a constraint — it never back-solves `duration`
  from a manually-set `end_date`.
- A frontmatter value for a field with `compute:` is a validation
  error, mirroring the aggregate manual-set conflict.
- Missing input → value absent; `error_on_missing: true` surfaces a
  diagnostic instead, matching the aggregate config.
- `schema.schema.json`: formal definition of the `compute` config.
- Terminology cleanup: the "Computed fields" comment blocks in
  `defaults/schema.yaml` (and this repo's `.workdown/schema.yaml`)
  describe aggregates — rename them to "Aggregated fields" so the term
  is free for this feature.

## Acceptance

- `end_date` with `compute: start_date + duration` shows the correct
  date in query, table, board, and gantt without appearing in any file.
- A computed leaf field with `aggregate:` config also declared rolls
  up to ancestors (computed-then-aggregated composition).
- Schema-load errors for: type mismatch between expression and field,
  unknown field or constant reference, expression cycles.
- Validation error when an item sets a computed field in frontmatter.
- Unit tests per operator per type pairing in the algebra.

## Out of scope

- Cross-item paths in expressions (`parent.rate`, `children.effort`) —
  inheritance cases decompose into defaults/resources plus a same-item
  computation.
- Bidirectional solving ("fill in whichever of end/duration is
  missing").
- Conditionals, functions, string operations.
- Writing computed values back to files, or an override syntax.
