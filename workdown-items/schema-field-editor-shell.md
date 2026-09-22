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
- **Save flow** (revised 2026-09-22). Save calls the write endpoint
  directly. A load failure (`422`) shows the parse error in the panel
  and nothing is written. On success the panel closes, any warnings
  the write produced show in the banner as for items and views, and
  the page refetches on the watcher's ping. No preview dialog.
- **Usage section.** On open, the panel fetches
  `GET /api/schema/fields/{name}/usage` and shows what depends on the
  field: config roles, views, rules and recipes by name, items as a
  count. The section degrades to "could not load" when the project
  does not load; the rest of the panel still works.
- **Remove.** Button in the footer. Greyed out, with the usage section
  as the explanation ("edit or delete view foo first"), while a role,
  a view, a rule or a recipe names the field. Items holding a value
  do not block.
- **Draft survives refetches.** The panel holds its draft until save.
  The watcher's ping refetches the page behind the panel and must
  never reset an open panel. No conflict detection: last write wins.

## Not in scope

- The numeric, text, value-list and relation blocks:
  [[schema-field-editor-numeric]], [[schema-field-editor-text]],
  [[schema-field-editor-values]], [[schema-field-editor-relation]].
- Changing an existing field's type: [[schema-field-type-change]].
