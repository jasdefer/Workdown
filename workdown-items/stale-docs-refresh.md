---
id: stale-docs-refresh
status: to_do
title: Fix the documentation that is actively wrong
parent: maintenance-review-2026-08
---

## In plain words

A few pieces of documentation do not just lag behind the code — they
say the opposite of what is true. The worst is the web UI's README,
which tells a new contributor the app is read-only and editing is not
built yet; all of that shipped long ago. Each fix is minutes; together
they remove every place the review found where reading the docs makes
you *less* correct about the code.

## The problem in detail

- **`ui/README.md:4,22`** — says the SPA "renders a project's views
  **read-only**" and "Mutations, item detail pages, and live
  file-watching are … not implemented yet". All three shipped. This is
  the first file a UI contributor reads, and it misdirects them on
  line one.
- **`ui/vitest.config.ts:3`** — claims "the only tests so far are …
  the gantt view"; there are nine test files.
- **`crates/server/src/envelope.rs:14`** — references "the same shape
  as `workdown check --json`"; the command is
  `workdown validate --format json`
  (`crates/cli/src/cli/mod.rs:43-46`).
- **`crates/core/src/operations/frontmatter_io.rs:86-113`** — two
  merged doc comments sit on the wrong function: the block describing
  `parse_value_for_field` tops `parse_collection_values`, and
  `parse_value_for_field` (line 114) is undocumented. (Its collection
  arm also re-implements `parse_collection_values`' comma-split
  inline — fold that while there.)
- **`crates/core/src/parser/schema.rs:152,162,172`** — doc comments
  say "Regex for valid field names" over what are hand-rolled
  character loops, not regexes.

(The stale doc table inside `views_check.rs` was handled by
[[metric-row-check-unification]], which rewrote those functions;
nothing left to do there. `docs/views.md`'s count-with-value rule was
corrected in the same pass.)

## Objective

Every listed location corrected. For the README specifically: describe
what the app *is* today in a few sentences — views, item editing,
filters, live updates, the timer — not a feature list to maintain.

## Out of scope

- New documentation ([[render-flow-doc]] and [[web-layer-adr]] cover
  that).
