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
- Minimal expression syntax: `+ - * /`, parentheses, and numeric
  literals (`effort * 1.2`). No conditionals, no functions, no
  duration/date literals — named quantities belong in constants.
- Own expression module: lexer with source spans, small Pratt parser,
  typed AST stored on the field definition, type check as a separate
  pass. No parser-generator dependency for a ten-token grammar.
- Closed type algebra, checked against the schema at load time:
  `date ± duration → date`, `date − date → duration`,
  `duration ± duration → duration`, `duration / duration → float`,
  numeric arithmetic on `integer`/`float` (division → float),
  `number * duration → duration`. The expression's result type must
  match the field's declared type — anything else is a schema error.
- Date-valued results with a sub-day remainder honor an optional
  `round: nearest | floor | ceil` on the compute config (default
  `nearest`). Floor/ceil express scheduling intent: last fully-used
  day vs. the day the work spills into.
- Evaluation order: field-level dependency DAG (compute inputs →
  output), processed topologically, each field running compute then
  aggregate. Cycles among field expressions are a schema-load error.
- Compute + aggregate on the same field: compute fires only on items
  whose inputs are all manually set, and its result behaves exactly
  like a manual value — including the rollup chain-conflict
  diagnostic; aggregate fills all other items. This makes a
  milestone's `end_date` the `max` of its children (correct across
  gaps), not its rolled-up `start + duration`.
- A field with only `compute:` may consume rolled-up inputs. This is
  load-bearing for ratios: `flow_efficiency = effort / duration` on a
  milestone must be `sum / sum`, not an aggregate of children's
  ratios.
- One direction per field: `end_date: compute: start_date + duration`
  is a declaration, not a constraint — it never back-solves `duration`
  from a manually-set `end_date`.
- Manual override: a frontmatter value on a computed field always
  wins, silently — compute fills only absent values, mirroring how
  default generators behave.
- Missing input → value absent; `error_on_missing: true` surfaces a
  diagnostic instead, matching the aggregate config. Runtime failures
  (division by zero, date overflow) → item-level diagnostic, value
  absent.
- Schema-load errors for contradictory configs: `compute` + `default`
  is rejected; `compute` + `required` gets a post-pass presence check
  like aggregates already have.
- `schema.schema.json`: formal definition of the `compute` config.
- Terminology cleanup: the "Computed fields" comment blocks in
  `defaults/schema.yaml` (and this repo's `.workdown/schema.yaml`)
  describe aggregates — rename them to "Aggregated fields" so the term
  is free for this feature.

## Acceptance

- `end_date` with `compute: start_date + duration` shows the correct
  date in query, table, board, and gantt without appearing in any file.
- A computed leaf field with `aggregate:` config also declared rolls
  up to ancestors (computed-then-aggregated composition), and the
  aggregate — not compute — fills items with derived inputs.
- Schema-load errors for: type mismatch between expression and field,
  unknown field or constant reference, expression cycles,
  `compute` + `default`.
- An item's manual frontmatter value on a computed field is kept
  verbatim; compute never touches it.
- Unit tests per operator per type pairing in the algebra.

## Out of scope

- Cross-item paths in expressions (`parent.rate`, `children.effort`) —
  inheritance cases decompose into defaults/resources plus a same-item
  computation.
- Bidirectional solving ("fill in whichever of end/duration is
  missing").
- Conditionals, functions, string operations, duration/date literals.
- Writing computed values back to files.
- An `on_manual: warn | error` strictness flag for manual overrides —
  add only if silent override proves error-prone in practice.
