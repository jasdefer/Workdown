---
id: schema-field-reorder
status: done
parent: schema-editor-web
depends_on: [schema-page-read-view, schema-field-write-backend]
title: Move fields up and down on the schema page
---

## In plain words

Field order in `schema.yaml` is the order of the create form, the
detail panel and the board's columns. Today changing it means cutting
and pasting YAML blocks. This item adds Up and Down buttons to each
row of the fields table; a click writes the new order.

Decision 11 of [[schema-editor-web-design]], revised 2026-10-08 from a
drag handle to buttons.

## What is needed

- Up and Down buttons per row, as the value list in the field editor
  already has. The buttons stop the click from opening the row's
  editor.
- A click sends the full ordered name list to
  `PUT /api/schema/field-order` from [[schema-field-write-backend]]
  (not `/fields/order`, which a field named `order` would clash
  with). The server reorders the entries under `fields:` in the YAML
  tree; comments are not preserved (revised 2026-09-22).
- `id` stays first: its row has no buttons, the row below it cannot
  move up, and the list sent is the list as shown, so after the first
  move the file declares `id` first too. The server does not enforce
  the position; it means nothing in the file.
- One write and a refetch per click, as the board does it. The buttons
  are greyed out while a write is in flight, because a second click
  before the refetch would be computed from the old order and send the
  same move again. The pressed button gets keyboard focus back after
  the refetch, so Enter walks a row through the table.
- A refused write (a stale list, after another tab removed a field)
  shows above the table where field-save warnings go, then the page
  refetches so the list repairs itself. Warnings ride back the same
  way as for every schema save.
- No conflict handling beyond that: last write wins.

## Not in scope

- Drag and drop. The feature is rarely used; buttons need no drag
  code and work with keyboard and touch, which the native drag the
  board uses does not.
- Reordering rules, or values inside a choice field (the latter is
  [[schema-field-editor-values]]).

## Decisions taken

Settled on 2026-10-08 with the user.

1. **Buttons, not a drag handle.** Simple and clean for a feature used
   rarely. Drops the drag-and-drop helper change, the drop-position
   question and the touch-screen gap in one go; keyboard works for
   free.
2. **Buttons greyed out while a write is in flight.** The alternative,
   letting a fast double-click through and moving the row once, reads
   as a lost click.
3. **The file gets the order as the page shows it,** `id` first. The
   alternative, keeping `id` wherever the file had it, would leave the
   file and the page disagreeing about the one pinned row.
4. **Failures show above the table** beside the last save's warnings,
   not next to the buttons; one place for everything the schema page
   writes.
5. **The order logic is one pure function** beside the page's other
   helpers, with unit tests; the buttons are plain markup, untested as
   the board's drag wiring is.
