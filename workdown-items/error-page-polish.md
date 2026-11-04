---
id: error-page-polish
title: A centered error page with a 404 illustration
status: done
parent: misc-work
tags: [web-ui]
---

The route-level error page of the web UI is a left-aligned block of
text at the top of the content area: a status line, the message, a
hint. It does its job but looks like a placeholder next to the rest of
the app, and a wrong URL is the one page every reader eventually sees.

Asked for while reviewing the item dialog redesign (2026-11-02).

## Requirement

- The error page is centered in the content area, horizontally and
  vertically, in a column that reads well on a wide window.
- A 404 shows an illustration in the app's own vocabulary (a work item
  card that is not there), drawn as inline SVG from the theme's colour
  tokens so it follows light and dark mode.
- The three cases stay as they are: 404 points at the navigation, 422
  lists the project's diagnostics, anything else offers a refresh.

## Decisions taken (2026-11-02)

1. The illustration is one Svelte component (`ErrorArt`) with the
   status as its prop, so another status can get its own picture later
   without touching the page. Today only 404 has one.
2. Colours come from `currentColor` and the tokens, no literal hues.
3. No tests: presentation only, nothing to assert in Node.
