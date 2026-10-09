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
