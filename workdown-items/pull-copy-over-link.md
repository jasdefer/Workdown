---
id: pull-copy-over-link
title: Inherit any field type from a linked parent (`pull` over a `link` copies), and `eq_field` on choice
status: in_progress
parent: schema-expressions
depends_on: [pull-fields]
tags: [enhancement, bug]
---

## In plain words

A consumer project wants to write a few fields once on a root item and
have every descendant inherit them: a `reference` string, a `format`
choice, a `section` integer. Today only the integer can do that, with
`pull: { over: parent, field: section, function: max }`. The string and
the choice are rejected because `pull` is only valid on integer, float,
date, duration and boolean — the reduction function needs a type it can
reduce. So `reference` and `format` are copied by hand onto every item.

The fallback, a rule with `eq_field: parent.format`, silently passes
for `choice` fields: the comparison helper has no choice arm, returns
"incompatible", and the check skips instead of failing loudly as its
doc comment promises.

Reported as [workdown#62](https://github.com/planningio/workdown/issues/62),
refined in its first comment.

## Requirement

1. **`pull` over a single `link` copies.** With a `link` there is
   exactly one source item, so there is nothing to reduce. `function`
   is left out for a `link` source; writing one is a schema error.
   The copy works for every type the same way (string, choice,
   multichoice, integer, float, date,
   duration, boolean, color, list). The pulled field may not be `link`
   or `links`, for the same reason `when` rejects them: a derived link
   is a phantom edge. Over a `links` source `function` stays required
   with today's type restrictions.

   ```yaml
   reference:
     type: string
     required: true
     pull: { over: parent, field: reference }
   ```

   Everything else carries over: never written to the file, a
   hand-written value wins, `required` makes an unanchored root a
   diagnostic, cascades through the dependency graph.

2. **`eq_field` on `choice`.** Equality is well defined for choice
   values (for every value, in fact), so `eq_field` compares them.
   The ordered comparisons stay undefined on choice; rejecting them at
   schema load is [[rule-comparison-reference-check]].

Touches: the schema check for `pull` (type and function validity), the
pull evaluation, `compare_field_values` in `crates/core/src/rules/assertion.rs`,
`schema.schema.json`, the schema editor's `PullBlock` in the web UI,
and the docs for `pull`.

## Decisions taken

Taken 2026-11-05.

1. **One `PullConfig`, `function` optional.** Not an enum of copy
   and reduce: which one applies is only known once `over` resolves
   to a `link` or `links`, so the config check decides either way;
   an enum would only add a union type to the schema editor.
2. **Copy over `link` only.** Over `links` nothing changes:
   `function` required, today's type restrictions.
3. **Same type only, plus integer into float**, the one widening the
   reductions already allow. Cross-type copies are
   [[pull-copy-type-conversion]]: each conversion is a small design on
   its own and the reporter needs same-type copies.
4. **`pull` parses on every type but `link`/`links`.** The parser can
   no longer reject `pull` by the pulling field's type alone, so the
   per-type gate moves to the config check, which knows the relation
   kind. `link`/`links` stay rejected for the same reason `when`
   rejects them: a derived relation is a phantom edge.
5. **No `ne_field`.** Not in the code, not asked for; `not` with a
   literal covers the need.
6. **No new load-time check for comparisons.** Rules get none today;
   a check for the comparison operators alone would be a special case.
   Instead the evaluator's helper splits in two: equality is answered
   for every type, so `eq_field` can never skip for a type reason;
   the ordering helper keeps serving the four ordered operators.
   Rejecting an impossible comparison at load is
   [[rule-comparison-reference-check]].
7. **Rule editor offers equality on every type.** The server serves
   two type lists, equality-defined and ordering-defined; the
   requirement row offers "equals" where equality is defined and the
   ordered comparisons only where an order is.
8. **Pull block hides "Combine with" over a `link`.** There is no
   choice to make, so the picker goes and the hint reads "An item's
   value is `field` on the item its `relation` points at".
