---
status: to_do
parent: schema-expressions
title: Compare a field with a literal in a rule's match block
---

## In plain words

A rule can say "where status is open" but not "where points is
greater than 5". The match grammar has equals, one-of, not, is-set and
the quantifiers; the only comparisons in the language are the require
side's field-against-field ones (`lt_field: end_date`). Asked for on
2026-10-09 while clicking through the rules editor ([[schema-rules-editor]]):
an integer field's test picker offers no `>`, because the file could
not hold one either.

## What to settle

- **Operator keys.** `gt`, `gte`, `lt`, `lte` beside `not` and
  `is_set` in the condition object, each taking one literal; or a
  `min`/`max` pair. The view filter language already has
  `operators_by_type`; the rule grammar should spell the same
  operators so the two read alike.
- **Types.** Integer, float, date, duration — the ordered types the
  require comparisons accept; dates coerced as equality already is.
- **Reach.** Parser (`condition_from_value`, type check against the
  target field), evaluator, `rule_shape` rows (a `ScalarTest`
  variant, exhaustive matches carry it to the editor), `docs/schema.md`,
  `schema.schema.json`. The editor then offers the comparison only on
  those types, as the require picker does.
