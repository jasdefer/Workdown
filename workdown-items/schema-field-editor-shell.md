---
id: schema-field-editor-shell
status: to_do
parent: schema-editor-web
depends_on: [schema-page-read-view, schema-field-write-backend]
title: The field editor panel, with the shared header and the save flow
---

## In plain words

Click a field on the schema page and a panel slides over, the same way
an item does. Change what the field is called, whether it is required,
its description, its default, and save. Saving shows what the change
would do to existing items before it writes. This item builds that
panel with everything every type shares, plus the simplest
type-specific block — the types with no extra properties at all. The
other blocks are one item each.

The field editor as defined in [[schema-editor-web-design]].

## What is needed

- **Open and create.** A row click opens the panel for that field; the
  **Add field** button opens it empty.
- **Header block.** Name (free for a new field, locked for an
  existing one), type selector (every type for a new field; for an
  existing field only the current type until
  [[schema-field-type-change]]), required toggle, description.
- **Default control.** Three-way: none, a fixed value entered with the
  same type-dispatched editor the item panel uses, or a generator
  chosen from those the API lists as valid for the type.
- **Plain-scalar block.** color, boolean and date have no type-specific
  properties; the block is empty. This proves the shell before the
  richer blocks arrive.
- **Derived fields.** A `compute`/`when`/`pull`/`aggregate` block is
  shown read-only as YAML with "edit in schema.yaml" beside it, and the
  type selector is locked. Everything else stays editable.
- **`id`.** Type locked, no remove button.
- **Save flow.** Save calls the preview endpoint. A load failure shows
  in the panel and nothing is written. New warnings open a confirmation
  dialog listing them by count and by item, in the commit dialog's
  style; confirm writes, cancel returns to the panel. No new warnings
  writes directly. The panel closes on success and the page refetches
  on the watcher's ping.
- **Remove.** Button in the footer, same preview and dialog.
- **`409`.** The panel says the file changed on disk and offers to
  reload the field, discarding pending edits.
- **Comment note.** When the field's entry carries comments, one line
  above the footer says they will not be kept.

## Not in scope

- The numeric, text, value-list and relation blocks:
  [[schema-field-editor-numeric]], [[schema-field-editor-text]],
  [[schema-field-editor-values]], [[schema-field-editor-relation]].
- Changing an existing field's type: [[schema-field-type-change]].
