---
id: item-detail-side-placement
title: Let the item detail open at the side again for quick browsing
status: to_do
parent: misc-work
depends_on: [item-overlay-redesign]
tags: [web-ui]
---

## In plain words

Since [[item-overlay-redesign]] an item opens as a centered modal. That
is the right default: the detail finally gets room, and the item is
marked in the view behind it. But it is the wrong shape for one common
way of working — clicking through many items in a row to get an
overview. Three things make it worse than the old side panel:

- **Every click covers the view**, so the reader loses sight of where
  they are in the board or table behind it.
- **Each item costs one click more.** The modal has to be closed before
  the next card can be clicked at all, so browsing ten items is twenty
  gestures instead of ten. With the side panel the next card was simply
  clicked and the panel showed it.
- **The modal feels slower to open** than the slide-in did. Whether
  that is the dialog's animation, the item fetch, or something else is
  not known yet; it is noticeable when opening items in quick
  succession.

The old side panel was better at exactly this: the view stayed visible,
the next card was one click away, and the detail was glanced at rather
than read.

## Requirement

Browsing many items quickly from a view should be cheap again, without
giving up the centered modal as the default for reading one item
properly.

## One idea for the implementation (a suggestion, not a decision)

A control that switches the detail to a side placement, like the panel
before the redesign, as an optional mode the reader can turn on and
off. Points to consider if it goes that way:

- **Body first.** In a narrow side panel the frontmatter fields push
  the Markdown body far down, and the header on top is often long. In
  the side placement the body would be a better thing to show first,
  with the fields after it (or collapsed), the opposite of the
  two-column modal.
- **The view stays usable** while the panel is open: clicking another
  card replaces the panel's item instead of closing and reopening.
- **The choice should stick** for the session at least, so the reader
  does not toggle it on every item.
- The docked placement still exists as `DockedPanel` in `lib/ui` (the
  schema editor uses it), so the side placement does not have to be
  rebuilt from scratch.

Any other approach that makes quick browsing cheap is fine too; the
toggle is only the first shape that came to mind.
