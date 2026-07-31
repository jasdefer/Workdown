---
id: where-clause-value-validation
type: issue
status: done
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

[[explicit-in-operator]] has since made this sharp rather than
theoretical. A clause that used to be an IN filter (`type=milestone,epic`)
is now a literal equality against the string `milestone,epic`, so any
project that hand-wrote one already has a silently empty view and no
diagnostic explaining it. Validating operands is what turns that from a
mystery into a message.

## Scope

- For `choice` / `multichoice` fields, check the operand against the declared
  `values`. For `link` / `links`, check it resolves to a known item id. For a
  `resource:`-backed field, check it against the section's entries.
- Applies to the operators where "is this a known value" is a meaningful
  question: `=`, `!=`, `in`, `not in`, and membership `contains` on
  collections. Not `matches` (a regex operand is not a value), not the
  presence checks. Ordering comparisons carry no *value set*, but they do
  carry a type — see the parse check below.
- For the non-enumerable scalar types (`date`, `boolean`, `integer`,
  `float`, `duration`) check the operand *parses* as that type, on the
  ordering comparisons too — a `start_date > yesterday` fails exactly the
  way `status=nonsense` does.
- Save-with-warning per ADR-001 — a bad operand warns, it does not block a
  write or a render. New diagnostic kind carrying the view id, the clause, the
  field and the offending value.
- Same treatment for a metric row's per-row `where:`, which has its own
  parallel check path, and for an ad-hoc `workdown query --where`.

## Acceptance

- A view filtering `status=nonsense` reports a diagnostic naming the field,
  the value, and the values that would be valid; `workdown render` still
  writes the view.
- A stale `type=milestone,epic` under the post-[[explicit-in-operator]]
  grammar produces that diagnostic rather than an empty view.
- A `matches` clause with a regex operand produces no value diagnostic.
- The filter editor's save path reports the same finding the persisted
  file would, so a bad operand is caught as it is written.
- `workdown query --where status=nonsense` explains the empty result on
  stderr; piped table/JSON/CSV output stays clean.

## Decisions taken

1. **One rule everywhere.** All three option sets are checked on every path
   that handles a `where:` clause — the persisted file, the filter-editor
   write, and `query --where`. `view_write` loads resources and the item
   set the way `add`/`set` already do; the neighbouring view endpoints
   already pay a full project load per request, so the write path was an
   inconsistency, not a fast path worth protecting.
2. **Item ids are checked**, at warning severity. A filter naming an item
   that does not exist yet is a legitimate forward reference, but a typo is
   far likelier and a warning blocks nothing.
3. **Warnings must stop hiding views.** `render`'s `invalid_view_ids` and
   the server's tier-2 gate both treat *any* view-pinned diagnostic as
   "this view cannot render". This item introduces the first warning-severity
   view diagnostic, so both gates move to `severity == Error` first —
   otherwise the warning would hide the view outright, which is worse than
   the silent-empty-view it reports.
4. **Parse checking is in scope** for the non-enumerable scalars, per the
   resolved open question below.

## Settled since filing

- [[resource-option-lists]] landed, so the `resource:` constraint is in
  scope: a resource-backed field has a checkable option set like any
  enumerable type. Reuse `resources_check::validatable_fields` rather
  than re-deriving the rule — it already decides when a section is worth
  checking against (a missing or empty one is reported once at schema
  scope and stops per-value checking), so a view filtering on an
  unpopulated resource stays quiet here for the same reason items do.
- The boolean/date open question resolves as *in scope*. The user-visible
  failure is identical — a filter that silently matches nothing — and the
  sharpest remaining case is a mistyped date in a gantt filter, which a
  value-set-only check would never reach.

## Settled since filing

- [[resource-option-lists]] landed, so the `resource:` constraint is in
  scope: a resource-backed field has a checkable option set like any
  enumerable type. Reuse `resources_check::validatable_fields` rather
  than re-deriving the rule — it already decides when a section is worth
  checking against (a missing or empty one is reported once at schema
  scope and stops per-value checking), so a view filtering on an
  unpopulated resource stays quiet here for the same reason items do.
