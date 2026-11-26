---
id: card-preview-links-dead
title: Links to other work items are broken in the card preview
status: done
parent: misc-work
tags: [web-ui, bug]
---

## In plain words

A card in a view shows the first lines of the item's description. When
that description points at another item, the preview shows a link, and
clicking it lands on a 404 instead of on the item. Opening the card
first and clicking the same link in the detail works. Reported as
GitHub issue #69.

## What is actually happening (read from the code, 2026-11-17)

[[item-body-links-dead]] fixed item references in the item detail and
nowhere else. The fix is a `marked` extension (`itemLinkExtension` in
`lib/items/bodyLinks.ts`) that the host has to hand to the `Markdown`
component, and only `ItemEditor` does. The card (`lib/views/board/
Card.svelte`, reused by the gantt, graph and treemap views) renders the
body with no extension, so in the preview:

- **`[text](other-item.md)` is a relative link.** The browser resolves
  it against the current route, `/views/<view>/other-item.md`, which
  does not exist. This is the 404 from the report, `.md` suffix
  included.
- **`[[id]]` is plain text.** It is not broken, but it is not a link
  either, so the same reference reads differently in the preview and in
  the detail.
- **The click also reaches the card.** The whole card is a button that
  opens its own item, and a link inside it does not stop the click, so
  whatever a preview link does, the card's own open fires as well.

## Requirement

A reference to another work item behaves the same in the card preview
as in the item detail: both spellings resolve, a click opens the
referenced item in the view's detail (`?item=`), and a reference to an
id no item has reads as dead text rather than leading to a 404. A click
on a preview link opens the referenced item only, not the card's own.
