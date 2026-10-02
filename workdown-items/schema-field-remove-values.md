---
id: schema-field-remove-values
status: done
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
  the `unset` mutation uses. The items are the ones whose file holds
  the key. Schema first, then items; the warnings the outcome reports
  are the ones that remain after both, and the outcome names the
  items it rewrote.
- **Server.** `DELETE /api/schema/fields/{name}?drop_values=true`
  takes the flag. One contract test for the flag.
- **Panel.** The confirm step gets the second button with the item
  count from the usage section. With no items holding a value there
  is one button, as today.
- **Undo** is a git revert; with the git pill on, schema and items
  land in one commit.

## Not in scope

- A CLI counterpart; the schema editor has none (design decision 7).
- Rewriting values on a type change or a value rename.

## Decisions taken

Settled on 2026-10-02 with the user.

1. **Schema first, then the items, through the file writer directly.**
   The `unset` operation looks the field up in the schema before it
   touches a file, so after the schema write it would refuse every
   item as "unknown field". The removal calls the frontmatter writer
   underneath it with the schema it still holds. A failure midway
   leaves the field removed and the untouched items with the warning
   they would have had anyway, the safer half-state; items first would
   risk values gone while the field still exists.
2. **"Holding a value" means the file's frontmatter has the key**, an
   empty value included. Each item file is read and split the way
   `unset` reads its target, so the test matches what produces the
   unknown-field warning.
3. **A query flag, not a request body.** A `DELETE` with a body is
   unusual; the views and git routes already take flags as query
   parameters. Absent means today's behaviour.
4. **The outcome names the rewritten items.** The panel does not need
   the list, but it is the only record of what was touched and a test
   asserts it directly.
5. **A second action button on the shared confirm dialog**, optional,
   styled like the confirm. Two labelled buttons say what each does; a
   checkbox would not. With no item holding a value the dialog has one
   button, as before.
6. **A file that fails to write is skipped, the rest continue.** It
   still holds the key, so the reload after the rewrite reports it with
   the ordinary unknown-field warning; the disk error goes to the
   server log. No new error shape for a case nobody will hit often.
7. **Rewritten items lose frontmatter comments and come back in schema
   order.** Every `set` already does this to the item it touches; the
   order is deterministic (schema order, then unknown keys
   alphabetically), so editing a file twice does not churn git. Said
   in the module docs, not in the dialog.
8. **Tests per the testing strategy.** Core: the happy path with the
   flag, an item without the key left byte-identical, the flag off
   leaving items alone. Server: one contract test with the flag. Web
   app: a unit test for the pure function that words the dialog.
9. **Remove sits beside Cancel, behind a thin vertical rule** (after
   the first review). Pushed to the footer's far edge it landed under
   the usage column of the wide panel and was easy to miss; the
   confirmation popover already guards the click, so distance from
   Save bought nothing.
