---
id: item-overlay-redesign
title: Redesign the work item overlay in the web UI
status: in_progress
parent: misc-work
tags: [web-ui]
---

Clicking an item inside a view opens its detail as a slide-over panel
at the right edge of the content area (`SlideOver` in overlay placement,
28rem wide). On a large screen the important details are squeezed into a
narrow strip at the edge while most of the window shows the view the
reader just left. The clicked item is not marked in the view either, so
once the panel is open nothing says which card, row or node it belongs
to.

Reported as [workdown#63](https://github.com/planningio/workdown/issues/63).

## Requirement

Redesign how an item's detail is presented when opened from a view.

- The detail gets the room it deserves on a wide window. One idea from
  the issue: a modal overlay in the middle of the screen with two areas,
  one for the frontmatter fields and one for the Markdown body.
- The item that was opened is marked in the view behind the overlay.
- What exists stays reachable: the standalone `/items/:id` page, the
  timer slot, field editing with diagnostics, the `?item=` URL state
  that opens and closes the detail.

## Decisions taken (2026-11-02)

1. **Form:** a centered modal on a native `<dialog>` opened with
   `showModal()`, the pattern the commit dialog already uses: top
   layer, inert page, Escape and focus return for free. On a wide
   window two columns, the frontmatter fields on the left and the
   Markdown body on the right, the body column the wider one; on a
   narrow window stacked and filling the window. The header and the
   timer pill are not reachable while an item is open; the item's own
   timer slot covers the timer for the open item.
2. **Marking, two tiers:** the open item (from `?item=`) gets a strong
   accent ring, the last opened item a thin, muted one after the
   dialog closes. "Last opened" is session-local, project-wide, and
   lasts until the next item is opened or the page reloads, so it
   follows into another view that shows the same item. Every
   item-presenting kind marks: board card, table row, tree node, gantt
   bar, graph node, treemap tile. The tier is a pure function of three
   ids, tested in Node.
3. **Standalone page `/items/:id`:** the shared `ItemEditor` lays
   itself out by its container, so the page gets the same two columns
   and a wider content area for free. No separate path.
4. **Body editing:** not in this item. The server has no body endpoint;
   a later item adds the endpoint and an editor.
5. **Closing and URL state:** `?item=` stays the truth. Escape, the
   close button and a click on the backdrop close; focus returns to the
   element that opened the item.
6. **`SlideOver`:** the overlay placement has no user left and is
   removed; the docked placement the schema editor uses is untouched.
   The schema field and rule editors stay docked beside their table.
7. **Tests:** per the testing section of `docs/architecture.md`, a
   Vitest unit test for the mark module (every input shape); nothing
   for the dialog mechanics the browser owns; no server change, no
   server test. A browser click-through in the container is the visual
   check before the commit, not a test in the repo.