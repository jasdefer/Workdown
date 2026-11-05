---
status: to_do
parent: misc-work
title: Check comparison references in rules at schema load
---

## In plain words

A rule can say "end date must be after start date" with
`gte_field: start_date`. The schema parser checks that `end_date`
exists, but not that `start_date` does. A typo there loads fine and
the comparison silently never fires, because the evaluator skips a
comparison whose other side resolves to nothing.

Found on 2026-10-09 while designing [[schema-rules-editor]]; the
editor's picker closes the gap from the browser, this closes it in
the parser so a hand-edited file gets the same refusal.

## What to do

In `validate_rules` (crates/core/src/parser/schema.rs), for each of
`eq_field`, `lt_field`, `lte_field`, `gt_field`, `gte_field`: the
value must be `$today` or a valid field reference (same checks as a
`require` key), and its target type should be comparable with the
asserted field's type. Same error shape as the other rule errors.

## Also cover: a comparison the type cannot answer

Added 2026-11-05 from [[pull-copy-over-link]]. That item splits the
evaluator's comparison helper in two: equality is answered for every
type, an ordering only for the types that have one. An ordered
comparison (`lt_field` and the rest) on a type without order, or a
comparison across two types that do not compare, still skips silently
at evaluation. It was deliberately left to this check rather than
given a check of its own, so rules get one load-time check with one
error shape. Reject it here next to the unknown-reference case:
`ordering_defined` in `crates/core/src/rules/assertion.rs` is the
answer for the ordered operators; `eq_field` needs the two sides to be
the same type (integer and float count as the same).
