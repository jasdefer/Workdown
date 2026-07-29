---
id: where-clause-value-validation
type: issue
status: to_do
title: Validate where-clause operands against the field's value set
parent: polish
depends_on: [explicit-in-operator]
effort: "4h"
---

A view's `where:` clauses are validated for *field names* only. `views_check`
resolves each reference against `schema.yaml` and reports an unknown field or
an unresolvable relation — then stops. The operand is never looked at, so
`status=nonsense` on a `choice` field with no such value validates cleanly
and matches nothing. The view renders empty and nothing explains why.

This surfaces sharply once [[explicit-in-operator]] lands: a clause that used
to be an IN filter (`type=milestone,epic`) becomes a literal equality against
the string `milestone,epic`. Any project that hand-wrote one gets a silently
empty view with no diagnostic. Validating operands is what turns that from a
mystery into a message.

## Scope

- For `choice` / `multichoice` fields, check the operand against the declared
  `values`. For `link` / `links`, check it resolves to a known item id.
- Applies to the operators where "is this a known value" is a meaningful
  question: `=`, `!=`, `in`, `not in`, and membership `contains` on
  collections. Not `matches` (a regex operand is not a value), not the
  ordering comparisons, not the presence checks.
- Save-with-warning per ADR-001 — a bad operand warns, it does not block a
  write or a render. New diagnostic kind carrying the view id, the clause, the
  field and the offending value.
- Same treatment for a metric row's per-row `where:`, which has its own
  parallel check path.

## Acceptance

- A view filtering `status=nonsense` reports a diagnostic naming the field,
  the value, and the values that would be valid; `workdown render` still
  writes the view.
- A stale `type=milestone,epic` under the post-[[explicit-in-operator]]
  grammar produces that diagnostic rather than an empty view.
- A `matches` clause with a regex operand produces no value diagnostic.

## Open questions

- Boolean and date operands are coercible rather than enumerable — worth
  checking they *parse* here, or does that belong with a broader
  operand-typing pass?
- The `resource:`-backed constraint is orthogonal to field type; whether
  operand checking honours it depends on [[resource-option-lists]] landing
  first. Probably yes, but confirm the ordering.
