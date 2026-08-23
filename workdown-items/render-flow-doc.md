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

`docs/` contains the ADRs plus `schema.md` and `views.md` (both
verified in sync with the code); CLAUDE.md maps the crates. There is
no "how `workdown render` flows" narrative: config → schema parse →
store load and coercion → derive graph → checks → `view_data`
extraction → renderer (CLI) or endpoint (server). The module headers
individually are excellent — the page should link them, not duplicate
them.

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
