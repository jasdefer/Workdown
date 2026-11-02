---
id: pull-copy-over-link
title: Inherit any field type from a linked parent (`pull` over a `link` copies), and `eq_field` on choice
status: to_do
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
   becomes optional for a `link` source and, if written, must be `copy`;
   anything else is a schema error. The copy works for every type the
   same way (string, choice, multichoice, integer, float, date,
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
   values, so `eq_field` / `ne_field` compare them. The ordered
   comparisons stay rejected, and an unsupported comparison is a schema
   load error rather than a silent pass.

Touches: the schema check for `pull` (type and function validity), the
pull evaluation, `compare_field_values` in `crates/core/src/rules/assertion.rs`,
`schema.schema.json`, the schema editor's `PullBlock` in the web UI,
and the docs for `pull`.
