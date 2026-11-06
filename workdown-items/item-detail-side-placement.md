---
id: item-detail-side-placement
title: Let the item detail open at the side again for quick browsing
status: in_progress
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

## Decisions taken (2026-11-06)

1. **A switch, the modal stays the default.** The centered modal is
   unchanged. A button in the detail's header docks the item to the
   side; from then on every item opens there until the reader switches
   back. The side placement is `DockedPanel` (`narrow`, 28rem) in a
   flex row beside the whole view page, toolbar and bars included, so
   the view shrinks and nothing is covered. A non-modal dialog and a
   next/previous navigation inside the modal were considered and
   dropped: the first is a worse docked panel, the second solves the
   gesture count but not the covered view and needs an item order per
   view kind.
2. **Where the choice lives.** In the detail's header, in both
   placements, and in localStorage project-wide (one key, not per
   view, like the theme). Not in the URL: a placement is a reading
   habit, not a location. Switching with an item open moves that item
   into the other chrome at once; `?item=` stays the one truth.
3. **Order in the side panel: header, fields, body.** The frontmatter
   fields stay above the body: `collapsed: true` already trims them to
   the important ones, and the body can be very long. That is the
   editor's existing stacked layout below 52rem, so `ItemEditor` is
   untouched.
4. **Closing the panel.** The close button, and Escape while the panel
   is open, unless the key went to an open `<dialog>` (the timer's
   confirmation popover). Both route through the URL as in the modal.
5. **No view refetch on open.** `?item=` moves out of the page's
   `load()` into the page component (`page.url`), so opening or closing
   an item no longer refetches and re-renders the whole view before the
   detail starts loading. This was in the path for the old slide-over
   too; whether it is what made the modal feel slow is not measured. If
   the modal still feels slow after the click-through, a new item
   records the remaining finding.
6. **Both chromes share one header part** (standalone link, placement
   switch); the close button is the dialog's own or the panel's own.
7. **Tests.** A Vitest unit test for the placement module (default,
   read, write, unknown value, no storage). Nothing for the panel
   mechanics. A browser click-through in the container is the visual
   check before the commit.
