---
id: derived-field-single-predicate
title: Stop pull-filled required fields from raising false missing-value errors
status: done
parent: maintenance-review-2026-08
---

## In plain words

A required field normally has to be written into the file. But a field
can also be filled in automatically — computed from other fields,
rolled up from children, derived by condition, or pulled from a linked
item — so the "you forgot this field" check has to wait and see
whether one of those filled it before complaining.

That check runs in two halves, an early one and a late one, and each
keeps its own list of the automatic mechanisms. The early list is
missing `pull`, the newest one. So a field that is both required and
pull-filled gets accused of being empty before the pull has run: a
false error when the pull succeeds, and a doubled or contradictory
error when it does not.

## The problem in detail

`crates/core/src/store/coerce.rs:69-73` defers the required check when
`aggregate`, `compute`, or `when` is configured — **but not `pull`**.
`required_check` in `crates/core/src/store/derive.rs:786-790` includes
`pull`. The two lists are meant to be exact complements; they are not.

What a `required` + `pull` field produces today:

| Situation | Today | Correct |
| --- | --- | --- |
| Pull fills the value | `MissingRequired` from coercion | no diagnostic |
| Pull yields nothing (unanchored root) | `MissingRequired` from coercion *and* from `required_check` | one `MissingRequired` |
| Link target incomplete, `error_on_missing: true` | `MissingRequired` *and* `PullMissingInputs` | `PullMissingInputs` only |

No test catches this: the derive tests build items in memory and never
run `coerce_fields`, so nothing exercises both halves together.

**Found while writing those tests:** the third row has a second cause,
independent of coercion. `required` + `pull` + `error_on_missing: true`
emits `PullMissingInputs` *twice* — once from the pull pass
(`derive.rs:435`, which is what `error_on_missing` asks for) and again
from `required_check` (`derive.rs:847`, which re-derives the same
missing inputs to give a better message than the generic one). The
existing unit test for this path sets `required` without
`error_on_missing`, so only one emitter ever fired in a test.

## Objective

- The early check defers for `pull` as it already does for the other
  three mechanisms.
- `required_check` stays quiet when the pull pass has already reported
  the same incomplete inputs against the same item.
- Tests that cross the coercion-plus-derive seam, one per row of the
  table above. They belong with the tests that drive the full project
  loader (`crates/core/tests/`), not with the in-memory derive tests
  that bypass the seam by construction.

## Out of scope

- Deduplicating the two lists, and the question of whether the check
  should be split at all — [[validation-phase-boundaries]] owns that.
  Tidying the split before deciding whether it survives would be work
  done twice; the tests here are the safety net that decision needs,
  which is why this item runs first.

## Decisions taken

1. **A field's `default:` does not count as "filled in
   automatically".** Defaults are stamped at `workdown add` time and
   never applied retroactively, so an existing file with an empty
   required field is a genuine error even when the schema declares a
   default. The `when:` config's own `default` is a different thing —
   it fills at load and is already covered.
2. **Fix the defect, do not tidy the structure.** See "Out of scope".
3. **The `over: parent` default is not touched here**, though one of
   its three copies sits a few lines away —
   [[over-default-single-definition]] owns it, and mixing two unrelated
   problems into one change makes both harder to review.
4. **The item's original premise was slightly wrong.** It named three
   sites duplicating the predicate. There are two.
   `derive_fields_in_order` (`derive.rs:326-330`) looks similar but
   answers a different question — *which* mechanism applies, not
   *whether* one does — because the same-item and pull passes are
   gated and scheduled separately. Merging it in would lose
   information, not remove duplication.
5. **When both emitters fire, the pull message wins and the required
   check yields.** `PullMissingInputs` names the incomplete input and
   already implies the field is empty; the generic missing-required
   message adds nothing. So the item gets one message, not two, and
   not a second copy of the same one.
