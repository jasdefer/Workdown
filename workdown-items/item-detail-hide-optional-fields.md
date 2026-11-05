---
id: item-detail-hide-optional-fields
title: Let the reader hide optional fields in the item detail
status: done
parent: misc-work
depends_on: [item-overlay-redesign]
tags: [web-ui]
---

## In plain words

The item detail shows every schema field except `id`, whether or not the
item has a value for it and whether or not the reader cares. Helper
fields in particular get in the way: a condition-derived `color`, a
duration the timer writes to, a tag list nobody set. They take the same
room as title and status and make the field column long before the
reader reaches what they actually opened the item for.

## Requirement

The reader should be able to hide optional fields in the item detail
(the modal from a view and the standalone `/items/:id` page alike) so
that the fields that matter are the ones on screen. Required fields
always stay visible. Hidden fields must stay reachable: a hidden field
that needs a value still has to be editable, and a diagnostic on a
hidden field still has to be visible.

## Things to settle

- **What counts as optional.** The schema's `required: false` is the
  obvious reading. Fields that are derived (`when`/`compute`) are the
  typical "helper" case and could be hidden by default.
- **Where the choice lives.** A per-reader toggle in the detail
  (session or browser local) is the cheapest. A per-project setting
  would fit the existing `fields` display role, which views already
  use to choose their columns and card badges, but today that role
  does not reach the item detail; whether it should is part of this
  item.
- **Empty versus optional.** Hiding only fields that are both optional
  and empty is a different, smaller feature; decide whether that is
  enough.

## Decisions taken (2026-11-04)

1. **The schema says which fields fold, not a rule.** A field definition
   in `schema.yaml` takes `collapsed: true`. Nothing is derived from
   `required`, from `when`/`compute`, or from whether the item has a
   value: a mechanical rule cannot tell a tint color from an effort
   rolled up over children, and a positive "show these" list would
   silently drop every field added later. A negative list fails safe.
   Empty fields stay visible, so the reader sees directly what is not
   set.
2. **Collapsed, never hidden.** The item detail (modal and standalone
   page, through the shared editor) shows the unflagged fields as today
   and, at the end of the fields column, one row "Show N more fields"
   that unfolds the flagged ones in schema order, fully editable. No
   row when the schema flags nothing. The header is untouched.
3. **Detail only.** Views, `workdown render`, the display-role fallback
   and the CLI are not affected by the flag.
4. **Closed on every open.** No remembered state in the browser or the
   session; the fold keeps its state while the same item stays open,
   since the editor is not remounted on the reload after a save.
5. **Opens by itself** when a warning from the last save names a
   collapsed field, so a warning never points at something out of
   sight. Nothing else forces it open; a missing required field that
   is also flagged stays folded on open, because the detail shows no
   validation on open today.
6. **No restrictions.** A boolean on any field, required or derived
   included; no validation rule, no interaction with display roles.
7. **The flag's path:** the core field definition and its parser, the
   JSON schema, the schema guide's common options table, the schema
   endpoint's per-field entry the item editor reads, and the schema
   editor's read and write types with a checkbox beside "required" in
   the field form. The editor part is mandatory: it rewrites the file
   from its model and would otherwise drop the flag on the next save.
8. **Defaults.** The default schema `workdown init` copies flags
   `status_color`. This repo's own `color` field gets the flag only
   after the release is tagged, since the pre-commit hook runs the
   installed binary, which must know the key first.
9. **Tests.** Core: the parser round-trips the flag and it defaults to
   false. Server: the schema endpoint carries it and the schema
   editor's write keeps it. UI: one Vitest case for "a warning on a
   collapsed field opens the fold". Nothing for the fold itself.
10. **ADR-008** gets one paragraph under its consequences recording the
    single per-field presentation flag and why a negative list was
    chosen over a `fields` list.

## Decisions taken in review (2026-11-04)

11. **The new-item form folds too.** Same row, same label, closed when
    the form opens; the id override stays the last row of the grid, the
    unfolded fields slot in above it. Nothing opens the fold on its own
    there, since a create with warnings replaces the form with the link
    to the item.
12. **The schema table shows the flag** in a "Collapsed" column beside
    "Required", the same check mark, so a reader sees which fields fold
    without opening each row.
13. **One UI test for the round trip** of a set flag through the field
    draft into the write body, since decision 7 names that link as the
    one that would otherwise drop the flag on the next save.
