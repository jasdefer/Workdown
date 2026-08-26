---
id: view-kind-sync-guards
status: to_do
title: Make the non-Rust view-kind mirrors fail loudly when they drift
parent: maintenance-review-2026-08
depends_on:
- render-flow-doc
---

## In plain words

When a new view kind is added, the Rust compiler points at every Rust
place that must be updated. Three mirrors of that knowledge live
outside its reach: a hand-maintained table in the web UI, the JSON
editor-autocomplete schema, and the docs. The JSON schema turned out
to be guarded already — the gap there is narrower than the review
thought. The web UI table has nothing, and both existing guards check
*shapes* rather than the *list of kinds*, so a fourteenth kind missing
from a mirror still ships silently. Close that.

## The problem in detail

The three mirrors, and what guards each today:

- **`ui/src/lib/views/viewKinds.ts:1-10`** — a hand-maintained table
  of the thirteen view kinds with their slot and accepted-type lists,
  duplicating `views_check.rs` validation by convention. The blast
  radius is honestly bounded (the server re-validates, so drift is a
  UX gap, not corruption) and the file says so — but it is the one
  place the generated-types discipline is deliberately broken. Either
  add a sync test or serve the table from the backend, which already
  knows it.
- **`crates/core/defaults/views.schema.json`** — **already guarded**,
  contrary to the original review finding.
  `crates/core/tests/views_schema.rs` compiles the schema and runs it
  against the default `views.yaml`, a multi-view example, and a battery
  of bad shapes (29 tests). The remaining gap is narrower than "add a
  drift test":
  - the guard's example covers 11 of the 13 kinds, so two kinds are
    unexercised;
  - it validates *shapes*, never asserting that the set of `type:`
    values the schema accepts equals the `ViewKind` variants — a
    fourteenth kind absent from the schema passes every test.

  So: extend the example to all kinds, and add one assertion comparing
  the schema's accepted `type` values against the enum.
- **`docs/views.md`** — its view-kind table matched the `ViewKind` enum
  thirteen-for-thirteen when the review ran. A doc-drift test is
  optional; at minimum the adding-a-view-kind checklist
  ([[render-flow-doc]]) must name it.

## Evidence this is worth doing

`metric-row-check-unification` broadened the `count`-with-`value` rule
from metric rows to every aggregate slot, and the Rust change passed
every gate while `views.schema.json` and `docs/views.md` still
described the narrow rule — silent drift in exactly two of the three
mirrors this item is about, introduced by a change that was reviewed.
Both were corrected afterwards, and the JSON schema now carries the
rule once as a shared `$def` referenced by all three definitions, with
tests. The `viewKinds.ts` mirror had no equivalent stake in that
change and so was not exercised.

## Objective

A failing test (or backend-served data) for each mirror that can
drift silently, so the compiler-plus-tests together cover every
touchpoint on the adding-a-view-kind checklist.

## Out of scope

- A registry or trait-object architecture for view kinds — reviewed
  and rejected as premature; exhaustive enums plus guards are the
  right cost at thirteen kinds.

Depends on [[render-flow-doc]] because that item writes the
adding-a-view-kind checklist this one makes enforceable; writing the
checklist twice, or guarding a list nobody has written down, is the
failure mode.
