---
id: item-body-links-dead
title: Links to other work items do not work in the item detail
status: in_progress
parent: misc-work
tags: [web-ui, bug]
---

## In plain words

An item's description points at other items all the time — "follow-up
to [[git-sync-controls]]", "see [[burndown-chart]]" — and in the web
UI none of those pointers can be clicked. The reader has to remember
the id, close the detail, find the item in a view, open it. It is the
same in the dialog opened from a view and on the standalone
`/items/:id` page.

## What is actually happening (read from the code, 2026-11-03)

- **`[[id]]` references are plain text.** The body is rendered by
  `marked` with no extension for wikilinks, so `[[git-sync-controls]]`
  comes out as those literal characters. This repository's own items
  use the `[[id]]` form in over a hundred files; it has never been
  rendered as a link, so this is not a regression from
  [[item-overlay-redesign]], just never done.
- **Relative Markdown links go to the wrong place.** A link written as
  `[text](other-item.md)` is a relative URL, so the browser resolves
  it against the current route: `/views/<view>/other-item.md` or
  `/items/other-item.md`, both of which 404. Links to files outside the
  items directory (`../docs/architecture.md`) fail the same way.
- **Link fields cannot be followed.** `parent` and `depends_on` are
  rendered as a `<select>` to edit the value; there is no way to jump
  to the item the value names.

The server passes the body through untouched (`ItemDetail.body` in the
core crate's item projection), so wherever the fix lives, the raw text
is available to it.

## Requirement

A reference to another work item in the detail is a link that opens
that item:

- in the dialog, by swapping the dialog's item (the `?item=` state the
  dialog already runs on, see `itemHref` in `lib/items/itemLink.ts`);
- on the standalone page, by navigating to `/items/<id>`.

Both the `[[id]]` form and the `[text](id.md)` form should resolve, and
a reference to an id that does not exist should look different from a
working link rather than 404. Link fields should offer a way to follow
the value next to editing it. Links to anything that is not an item
(docs, external URLs) are out of scope here beyond not breaking them.
