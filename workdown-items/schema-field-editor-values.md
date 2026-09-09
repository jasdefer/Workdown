---
id: schema-field-editor-values
status: to_do
parent: schema-editor-web
depends_on: [schema-field-editor-shell]
title: Field editor block for choice and multichoice — the ordered value list
---

## In plain words

A choice field is its list of values. Adding `priority` with high,
medium and low is the example the whole feature started from (GitHub
issue #50). This is the block that edits that list: add a value, remove
one, rename one, put them in order. Order matters more than it looks —
it is the order of the board's columns and of every dropdown.

One of the five type-specific shapes in [[schema-editor-web-design]],
and the one with a real UI design question in it.

## What is needed

- An ordered list editor: add at the end, remove, drag to reorder,
  edit a value in place.
- Values follow the same identifier rules the parser enforces;
  duplicates are refused in the UI.
- **Renaming a value does not rewrite items.** Items holding the old
  value get a warning, and the save preview says how many. The block
  says so next to the edit control so nobody is surprised.
- Removing a value that the field's `default` names clears the
  default, with a note.
- A choice field currently in use by a board view: reordering values
  reorders the columns. Worth a line in the panel.

## Not in scope

- Renaming a value *with* item rewrite. A candidate follow-up if real
  use asks for it.
- Converting a string field into a choice: [[schema-string-to-choice]].
