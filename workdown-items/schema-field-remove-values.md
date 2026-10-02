---
id: schema-field-remove-values
status: to_do
parent: schema-editor-web
depends_on: [schema-field-editor-shell]
title: Offer to drop a removed field's values from every item
---

## In plain words

Removing a field from the schema leaves every item that holds it with
the key and an "unknown field" warning, to be cleaned up by hand.
Most of the time the values should go with the field. This item adds
that option: the remove confirmation offers "Remove field" and
"Remove field and its values", the second naming how many items it
rewrites.

Split out of [[schema-field-editor-shell]] (decision 8) so the core
change has its own tests and the panel item stays about the panel.

## What is needed

- **Core.** `remove_field` takes an option to also rewrite every item
  holding the key, dropping it from the frontmatter with the writer
  the `unset` mutation uses. The items are the ones the usage question
  already identifies. Schema first, then items; the warnings the
  outcome reports are the ones that remain after both.
- **Server.** `DELETE /api/schema/fields/{name}` takes the flag. One
  contract test for the flag.
- **Panel.** The confirm step gets the second button with the item
  count from the usage section. With no items holding a value there
  is one button, as today.
- **Undo** is a git revert; with the git pill on, schema and items
  land in one commit.

## Not in scope

- A CLI counterpart; the schema editor has none (design decision 7).
- Rewriting values on a type change or a value rename.
