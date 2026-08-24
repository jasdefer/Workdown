---
id: validation-phase-boundaries
status: done
title: Decide where the required-field check belongs in the load pipeline
parent: maintenance-review-2026-08
depends_on:
- derived-field-single-predicate
---

## In plain words

Loading a project runs as a fixed sequence of phases: read the files,
convert written values to their declared types, build the link graph,
run the fill-in mechanisms (compute, condition, pull, roll-up), then
check values against `resources.yaml`. The order encodes a principle —
a check that judges a field's final value runs after the fill-in
phase, so a derived value is held to the same standard as a
hand-written one.

One check does not follow that shape. "Is this required field filled
in?" runs in two halves: an early half, during type conversion, for
fields that no mechanism could fill; and a late half, after the
fill-in phase, for fields that something might have filled. It is the
only check that straddles the boundary, and the reason for splitting
it is not recorded anywhere.

This item is a decision to be taken, not a defect to be fixed. Nothing
misbehaves today on account of the split itself.

## The problem in detail

The two halves are in `crates/core/src/store/coerce.rs` (phase 3 of
`Store::load`, in `crates/core/src/store/mod.rs`) and in
`required_check` in `crates/core/src/store/derive.rs` (end of phase
5). Each decides whether a given field is its business or the other
half's by asking which fill-in mechanisms that field declares — the
same question, answered from two hand-maintained lists.

## What any answer has to account for

- **The pipeline's ordering principle.** Phase 6's resource check sits
  after the fill-in phase deliberately, and rule evaluation runs later
  still. Whatever is decided here should be consistent with why those
  are placed where they are, or should say why this check differs.

- **Both halves must be taught about every new mechanism.** There have
  been four so far. `pull`, the most recent, was taught to only one
  half — the defect tracked in [[derived-field-single-predicate]]. Any
  arrangement that keeps two decision points has to make disagreement
  between them impossible rather than merely unlikely.

- **The early half can distinguish two cases the late half cannot.** A
  value that fails type conversion is dropped from the item, so after
  the fill-in phase "never written" and "written but invalid" look
  identical — both are an absent key. Today a required field holding
  an invalid value produces exactly one complaint, about the value. A
  late check with no extra information would add a second complaint
  saying the field was not filled in, which is false. Carrying a
  per-item set of conversion-failed fields forward would resolve it;
  disabled compute fields are already threaded through the pipeline
  that way, so there is precedent for the mechanism.

- **The halves report in different orders.** The early half reports
  item by item, as files are read. The late half reports field by
  field, with items sorted by id inside each. Consolidating on either
  one changes the order of messages users and snapshot tests see, for
  projects that have a plainly forgotten required field.

- **Diagnostic quality should not regress.** The late half does more
  than report absence: it names the cause where it can — a computed
  field's missing inputs, a pull's incomplete link target — and falls
  back to the generic message otherwise. Whatever shape the check
  takes has to keep that specificity.

## Size, as measured

Taken before deciding anything, so the decision is not made in fear
of a refactor that is not there:

- The required-field diagnostic has 23 touchpoints in total, all
  inside `workdown-core`, across 7 files.
- Collapsing the check would change three source files —
  `store/coerce.rs`, `store/mod.rs`, `store/derive.rs` — plus their
  tests.
- Nothing outside core is affected. The CLI, server and web UI only
  render diagnostics; a changed order costs them no code.
- There is no golden-file snapshot framework in the repo, so no
  snapshot regeneration.

The one genuinely user-visible consequence is the message ordering
noted above. It is cosmetic, but it changes output for every existing
project with a plainly forgotten required field, so it wants a
deliberate decision and a changelog entry rather than arriving as a
side effect.

## What has to be decided

- Whether the required check becomes a single check after the fill-in
  phase, stays split, or takes some other shape.
- Whichever is chosen: how the points above are handled, and the
  reasoning recorded so the arrangement is intentional rather than
  inherited.

## Out of scope

- The `pull` defect itself. [[derived-field-single-predicate]] fixes
  that in isolation and pins the behavior with tests that cross this
  seam — the first tests to do so, and the reason this item is
  scheduled after it rather than before.
- Rule evaluation and view validation, which run outside the load
  pipeline.

Part of [[maintenance-review-2026-08]]: the milestone's common thread
is "make each fact live in exactly one place", and the hand-mirrored
mechanism list this item removes is one of the two facts that had
already drifted. It was originally kept outside the milestone so an
open architecture decision would not block completion; with the
decisions below taken, that reason no longer applies.

## Decisions taken

1. **The check becomes a single check after the fill-in phase.** The
   early half in coercion is removed entirely; the mirror-list
   invariant is not centralized but deleted — there is no second list
   left to disagree with. The check joins the resource check on the far
   side of the fill-in boundary, obeying the pipeline's ordering
   principle: a final value is judged only after everything that could
   produce it has run. Accepted cost: answering "is it missing?" now
   always needs the whole project loaded, so a future single-file lint
   could not reuse this check cheaply. Nothing needs per-file checking
   today; the CLI always loads the full project.

2. **"Which fill mechanisms exist" becomes one closed enumeration on
   the field definition** (aggregate, compute, pull, when). Every place
   that branches per mechanism — the check's cause-naming, the derive
   scheduler, the schema cross-checks — matches on it exhaustively, so
   adding a fifth mechanism is a compile error at every site that has
   not been taught about it. This turns "both halves must be taught
   about every new mechanism" from a review hope into a machine-checked
   guarantee, and it holds for consumers the check consolidation alone
   would not cover.

3. **Coercion records, per item, which fields were written but failed
   conversion, and that record is carried to the check** — the same way
   disabled compute fields already travel through the pipeline. The
   check stays silent for those fields: the invalid-value diagnostic
   already stands, and a second "missing" complaint would be false.
   This also fixes a latent defect the split never handled: for
   derivable fields, a written-but-invalid value already reached the
   late half looking like an omission — producing a false extra error,
   or being silently overwritten by the fill-in.

4. **Reporting order is item-first** (by item id, then field): users
   fix files, not schema fields, and it is the closest to what the old
   early half printed. Cosmetic but user-visible for every project with
   a forgotten required field — gets a changelog entry. Diagnostic
   specificity is preserved: the cause-naming messages (compute's
   missing inputs, pull's incomplete target, when's unmatched branches)
   move with the check.

5. **The load pipeline is restructured into named phase functions**,
   one per phase, each with its contract in a doc comment, with the
   ordering principle stated once at the top of the module. No generic
   stage framework — the phases are six, fixed, and heterogeneous;
   abstraction there would add indirection without removing any real
   mistake class. The compile-time safety comes from decision 2, not
   from scaffolding.

6. **The ordering principle is recorded as a slim ADR** (validation of
   final values runs post-derivation), since it is a rule future checks
   must obey, not just a note about this change. The concrete choices
   live here.

## Where it landed

- `FillMechanism` and `fill_mechanisms()` on the field definition —
  `crates/core/src/model/schema.rs`. `is_derived()` now matches on the
  enumeration, so a new mechanism must declare which side it falls on.
- The single check: `crates/core/src/store/required.rs`, its own
  pipeline phase between the fill-ins and the resource check.
- Coercion returns a `CoercionOutcome` carrying the per-item
  `conversion_failures` record; the fill-in phase refuses to fill
  recorded fields (no silent override of a broken hand-written value;
  aggregate contributions still pass through).
- The pipeline contract is stated once in the `store` module docs;
  `load` reads as six named phases. The ADR is
  `docs/adr/012-validation-after-derivation.md`.
- New seam tests in `crates/core/tests/computed_fields.rs` pin the
  invalid-vs-missing rows, the no-fill-over rule, the aggregate
  pass-through, and the item-first report order. Changelog carries the
  ordering change and the invalid-value fixes.
