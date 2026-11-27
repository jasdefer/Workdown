---
title: Workdown has no logo of its own
status: to_do
parent: misc-work
tags: [web-ui]
---

## In plain words

The browser tab of `workdown serve` shows the Svelte logo. It is the
favicon the SvelteKit project template shipped with
(`ui/src/lib/assets/favicon.svg`, linked from
`ui/src/routes/+layout.svelte`) and it was never replaced. A reader with
several tabs open sees the mark of the framework, not of the tool, and
the header shows the name as plain text only.

## Requirement

Workdown has its own mark, and the web UI uses it wherever the Svelte
logo appears today. No trace of the template's logo is left in the
repository.

## Ideas (suggestions, not decisions)

The implementer decides all of this after thinking it through.

- An SVG that stays legible at 16×16 pixels and works on a light and a
  dark tab bar. A simple shape reads better at that size than a
  wordmark.
- Motifs that fit the tool: a Markdown mark (the `#` heading or the
  down arrow of the Markdown logo), a check box, a card on a board, or
  a combination of a document and a check mark.
- The same mark next to the name in the header (`.brand` in the
  layout), in the README, and as the repository's social preview.
- Points to consider: whether the mark follows the UI theme or has one
  fixed color, whether a PNG or ICO fallback is needed for browsers
  that do not take an SVG favicon, and whether the tour page and the
  error page should show it as well.
