---
id: view-kind-sync-guards
status: to_do
title: Make the non-Rust view-kind mirrors fail loudly when they drift
parent: maintenance-review-2026-08
---

## In plain words

When a new view kind is added, the Rust compiler points at every Rust
place that must be updated. But three mirrors of that knowledge live
outside Rust's reach — a hand-maintained table in the web UI, the JSON
editor-autocomplete schema, and the docs — guarded today only by
"keep in sync" comments. They will drift the first time someone adds a
view kind under deadline. Add automated drift checks so forgetting one
fails a test instead of shipping a gap.

## The problem in detail

The unguarded mirrors:

- **`ui/src/lib/views/viewKinds.ts:1-10`** — a hand-maintained table
  of the thirteen view kinds with their slot and accepted-type lists,
  duplicating `views_check.rs` validation by convention. The blast
  radius is honestly bounded (the server re-validates, so drift is a
  UX gap, not corruption) and the file says so — but it is the one
  place the generated-types discipline is deliberately broken. Either
  add a sync test or serve the table from the backend, which already
  knows it.
- **`crates/core/defaults/views.schema.json`** — the editor
  autocomplete schema enumerates view kinds and their slots with no
  drift test. Precedent exists: `crates/core/tests/schema_schema.rs`
  already guards `schema.schema.json` against the serde model
  (ADR-005); the same pattern applies.
- **`docs/views.md`** — its view-kind table currently matches the
  `ViewKind` enum thirteen-for-thirteen, verified manually during the
  review; a doc-drift test is optional, but at minimum the
  adding-a-view-kind checklist ([[render-flow-doc]]) must name it.

## Objective

A failing test (or backend-served data) for each mirror that can
drift silently, so the compiler-plus-tests together cover every
touchpoint on the adding-a-view-kind checklist.

## Out of scope

- A registry or trait-object architecture for view kinds — reviewed
  and rejected as premature; exhaustive enums plus guards are the
  right cost at thirteen kinds.
