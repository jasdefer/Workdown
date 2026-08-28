---
id: render-flow-doc
status: done
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

## Decisions taken

Recorded 2026-08-28 after review.

1. **`docs/architecture.md`, linked from the contributor section.**
   The page is contributor-facing *how*, a third kind next to the
   user guides (`schema.md`, `views.md`) and the ADRs' *why*. It
   lives in `docs/` with the rest of the prose but is linked from
   README's "Working on workdown itself", not from the user-facing
   Documentation list. CLAUDE.md gets one line. Rejected: a root
   `CONTRIBUTING.md` (splits prose across two homes).

2. **One page, two sections.** The flow narrative and the
   adding-a-view-kind checklist ship together. The checklist is only
   meaningful to someone who has just read the pipeline it walks, and
   two thin pages create a "which one do I read" problem. Split later
   if the checklist grows.

3. **One shared spine, four exits — not the render path alone.**
   `project.rs` is explicitly the shared loader for render, validate
   and serve, so describing the spine as "the render flow" would send
   anyone debugging `serve` away from the page that answers their
   question. Narrative: config → schema → store load → derive →
   checks, then branching to the CLI renderer, the HTTP endpoint, the
   `validate` report, and the mutation write-back path.

4. **One Mermaid flowchart plus prose.** GitHub renders it and the
   repo already emits Mermaid from the gantt and graph renderers, so
   the format is native here. Diagram shows the spine and its exits;
   prose carries the per-stage detail.

5. **Links name files and symbols, never line numbers.** Stated as a
   rule at the top of the page so it outlives this change. Symbol
   names (`Store::load_with_resources_as_of`) survive edits and are
   greppable; line numbers would make this page the next entry in a
   future [[stale-docs-refresh]] — which is exactly how that item's
   references had to be repaired mid-milestone.

6. **The page owns the gaps between stages; module headers own the
   stages.** The dividing rule, stated on the page: a fact about the
   *order* of stages or the *hand-off* between two of them belongs
   here, because no single module can state it; a fact about what
   happens *inside* a stage belongs to that module's header and is
   linked, not summarized. Review test: a sentence that stays true
   when you delete the neighbouring stage is probably restatement.

7. **The checklist is a table of touchpoint → enforcement
   mechanism** (compiler / existing test / nothing yet), not a flat
   list of places. Every "nothing yet" row is then literally the
   backlog for [[view-kind-sync-guards]]. The table also records
   which layers need *no* change — the server is kind-agnostic, it
   serializes `ViewData` — since knowing what you can skip is half
   the value.

8. **Unwritten UI conventions become checklist rows.** The
   recording-dot comparison against `timerStore.runningItemId` that
   each item-presenting view implements independently is a row, with
   one sentence flagging six copies as an extraction candidate so the
   checklist does not quietly bless the duplication. Whether to
   extract it is separate work, not this item.

9. **No drift guard for the checklist in this item.**
   [[view-kind-sync-guards]] exists for exactly that, depends on this
   item, and its stated reason for existing is writing the
   compare-JSON-schema-against-Rust-enum helper once. Splitting the
   guard work across both items would reintroduce the duplication the
   milestone is removing.
