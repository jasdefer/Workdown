---
id: render-flow-doc
status: to_do
title: One page that shows how a render flows through the system
parent: maintenance-review-2026-08
---

## In plain words

Every stage of the pipeline is well documented where it lives, but
nothing connects the stages into one picture: config is read, the
schema is parsed, items are loaded and coerced, auto-filled fields are
derived, views are validated, view data is extracted, and finally the
terminal or the web app draws it. A newcomer has to discover that
order by reading module headers in the right sequence. One page with
that narrative — plus a checklist of everything a new view kind
touches — would cut ramp-up time materially. This was the only
genuinely *missing* documentation the review found.

## The problem in detail

`docs/` contains the ADRs plus `schema.md` and `views.md`; CLAUDE.md
maps the crates. There is no "how `workdown render` flows" narrative:
config → schema parse → store load and coercion → derive graph →
checks → `view_data` extraction → renderer (CLI) or endpoint (server).
The module headers individually are excellent — the page should link
them, not duplicate them.

Two things to build on rather than re-derive, both landed after this
item was written:

- `Store::load`'s middle is no longer prose to reconstruct. It is six
  named phase functions with the ordering contract stated in the
  module docs, and ADR-012 records the principle (a field's final
  value is judged only after everything that could produce it has
  run). Link them; don't restate them.
- ADR-006 now carries the ViewData/renderer dividing line — ViewData
  owns structure and order, renderers own wording and color — which is
  the last hop of the pipeline this page describes.

The page is also the natural home for the **"adding a view kind"
checklist**. Today a new kind touches roughly eight places — the
`ViewKind` enum, the views parser, `views_check`, a `view_data`
extractor plus the `ViewData` enum, a CLI renderer plus
`description.rs`, `defaults/views.schema.json`, a UI component, and
`docs/views.md` — and the compiler only enforces the Rust ones (see
[[view-kind-sync-guards]] for making the rest fail loudly). Unwritten
conventions belong on the list too, for example the recording-dot
comparison against `timerStore.runningItemId` that each of the six
item-presenting UI views implements independently.

## Objective

One page under `docs/` (for example `docs/architecture.md`): the flow
narrative with links to the module headers that own each stage, and
the adding-a-view-kind checklist.

## Out of scope

- Restating what module headers, schema.md, or views.md already say.
