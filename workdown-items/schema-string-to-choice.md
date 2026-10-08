---
id: schema-string-to-choice
status: on_hold
parent: schema-editor-web
depends_on: [schema-field-editor-values]
title: Turn a free-text field into a choice, with the values it already holds
---

## In plain words

The most common type change in a growing project: a field started as
free text, the same few values keep appearing, and now it should be a
dropdown. The type-change rules forbid it, rightly, because any value
not on the new list becomes invalid. But the tool can see which values
are in use — offer them as the starting list, and the change is safe
by construction.

**Parked.** The value-list editor it builds on has shipped; the
general type-change item it was to extend was removed on 2026-10-08
(the editor changes no types, see decision 12 of
[[schema-editor-web-design]]), so this would be the editor's only type
change and needs its own selector path. Noted in the design item as
the one forbidden conversion worth making possible.

## What has to be settled when it is picked up

- The distinct values in use come from the store; the editor
  pre-fills the value list with them and lets the user prune, rename
  or reorder before saving.
- Pruning a value in use is a warning the preview shows, like any
  value-list edit.
- Whether multichoice from list gets the same treatment.
