---
id: schema-field-editor-values
status: done
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

## Decisions taken

Settled on 2026-10-06 with the user. Three premises above were checked
against the code first: the parser enforced no identifier rule on
values, only that the list is not empty, so duplicates and blank
values went through; the save preview this item counted on was
dropped on 2026-09-22, warnings come after the write; and a `default`
naming a value no longer listed is a schema parse error, not a
warning.

1. **Duplicate and blank values are the parser's call.** A value
   listed twice would be two board columns for one option and two
   identical dropdown entries; blank text names nothing. Both are now
   refused by `parse_schema` beside "'values' must not be empty", so a
   hand-written list is held to the same rule as the editor. No
   identifier rule: values like `In Progress` are allowed today and
   stay allowed. The block refuses the same two things in place, with
   the reason, only so a save is never refused for something it let in.
2. **Up and Down buttons, no drag.** Native drag has no keyboard path
   and the app's drag helper only knows "drop on a target", not
   "insert before this row". Two arrows per row are enough for a list
   of a handful of values; drag can come later if use asks for it, and
   [[schema-field-reorder]] decides for the table on its own.
3. **Renaming does not touch items**, as the design fixed. Each value
   is its own text input, committed when it is left. The line under
   the list says items keep the old text and warn after saving; the
   warnings arrive through the existing save path, so nothing new was
   built to show them.
4. **No per-value item counts.** Showing "3 items" beside a value
   would need the usage endpoint to count by value. Left out until it
   turns out to matter; the warnings after the save say which items.
5. **Nothing automatic around the default.** A default naming a
   removed or renamed value is left alone; the server refuses the save
   with "default does not fit type 'choice': 'x' is not one of: …" and
   the user fixes it. The user chose this over the item's "clears the
   default, with a note" to keep the editor predictable.
6. **An empty list greys out Save** with "Add at least one value",
   through the same `shapeProblem` the bounds use.
7. **No board hint.** That reordering values reorders a board's
   columns is what reordering means; a line saying so would be noise.
8. **Add is an input under the list.** Enter or the *Add* button
   appends; Enter is kept from submitting the panel's form.
9. **`ValuesBlock.svelte` owns no draft state.** The pure helpers in
   `fieldDraft.ts` (`valueProblem`, `withValueAdded`, `withValueRenamed`,
   `withValueRemoved`, `withValueMoved`) carry the logic and the unit
   tests; the block keeps only the text being typed and the last
   refusal.
