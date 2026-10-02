---
id: schema-field-editor-shell
status: done
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

## Decisions taken

Settled on 2026-10-02 with the user.

1. **URL as state.** `/schema?field=<name>` opens the panel on a
   field, `/schema?add` opens it empty, as `?item=` does on view
   pages. Deep links and back-button close for free.
2. **The draft is never reset by a refetch.** The panel copies the
   field into a working copy when it opens and ignores later data.
   A refetch fires on every file change in the project, including a
   second tab, the effort timer and a pull, so resetting would lose
   work to background writes. A field removed meanwhile surfaces as
   the save's not-found answer; no special detection.
3. **One value editor for items and defaults.** The item panel's
   type-dispatched editor is given its own input type (the type, its
   constraints, whether required) and reports a plain value or null;
   the item panel wraps that into its mutation, the default control
   stores it as the fixed default. A refactor of the component and its
   one caller, done here, rather than an adapter bolted on.
4. **All twelve types in the Add form.** Most types are complete with
   no type-specific settings; a type whose block is a later item shows
   a line saying its settings are edited in the file for now. The two
   choice types cannot be saved until [[schema-field-editor-values]]
   lands; the server's "needs values" refusal explains it. No greying
   in the browser.
5. **Shape kind comes from the server.** Each entry of
   `properties_by_type` gains the shape kind the type uses, so the
   browser can send the right empty shape without a duration special
   case in TypeScript. Type knowledge stays in Rust.
6. **Warnings as a plain list.** The view banner partitions by view
   and fits nothing here. The page shows the item panel's
   "Warnings from the last change" list above the Fields table,
   replaced on the next save.
7. **Trust the server on names.** An invalid or taken name comes back
   as the server's message under the name input. No client-side rule
   to keep in sync.
8. **Removing values from items is its own item.** Most of the time
   the values should go with the field. The core option, the server
   flag and the second confirm button are
   [[schema-field-remove-values]], built directly after this one.
9. **Conditional fields keep their default locked.** For a field with
   a `when:` recipe the default is the recipe's fallback and the write
   leaves it untouched, so the control is disabled with that note.
   Recipe editing itself stays [[schema-derived-field-editor]].
10. **A wide panel docked beside the table, two columns inside**
    (2026-10-02, after the first review). The item panel's width
    wasted the page, and an overlay covered the table while there was
    room to the left. The panel chrome takes a `size` and a
    `placement`; the field editor is wide and docked: it sits beside
    the table in a row, the table shrinks to what is left, and nothing
    is covered. Inside, what is edited sits on the left and what is
    only shown, the recipe and the usage, on the right. Not a modal:
    the table stays visible and a click on another row swaps the panel.
11. **Usage by name, not by parser message** (2026-10-02, same
    review). The usage answer had reported a rule naming the field
    only as the parse error its removal would cause, and the panel
    printed that text. The server now names the rules and the
    `aggregate`/`pull` recipes referencing the field; the panel shows
    rules, views, config roles, recipes and the item count as labelled
    groups, and *Remove* says "remove the field from the 6 rules first".
    The parser's message is shown only when nothing named explains it.
