---
id: project-tour
title: Animated project tour in the web UI
status: in_progress
tags: [web-ui]
---

## In plain words

A newcomer opening `workdown serve` sees a board and has to build the
big picture themselves: how many items, how they nest, what blocks
what, where things stand. The tour tells that story in about a minute:
every work item appears as a card, and one set of cards flies through a
sequence of layouts — a scattered cloud, the headline numbers, the
hierarchy, the board columns, the dependency graph, the timeline — and
finally settles into the real default view, where the tour dissolves
and the interactive app takes over.

Not for item details; for the broader structure of them.

## The flow

One set of cards, many layouts. Every scene is "the same cards,
arranged differently, with a caption". Each scene is derived from
configuration that already exists; a scene whose source is missing is
skipped, never shown empty.

| Scene | Newcomer learns | Derived from | Skipped when |
|---|---|---|---|
| Title | what this is | `project.name`, `project.description` (already served for the browser tab) | never |
| Swarm | scale — cards in a 3D cloud, one slow camera pass | all items; card content via `defaults.display` roles | never |
| Numbers | size and health | the first `metric` view's rows; fallback: item count + the first board's column counts | never (fallback) |
| Structure | how work is organized | the first `tree` view | no tree view |
| Grouping | where things stand, who owns what | the first `board` view; one more scene for a second board on a distinct field | no board view |
| Dependencies | what blocks what; most-depended-upon item highlighted | the first `graph` view | no graph view, or no edges after its filter |
| Timeline | when; a "today" marker | the first `gantt` view (start + end/duration already resolved by the server) | no gantt view |
| Landing | "now go explore" | the first view in `views.yaml`, the same one `/` redirects to | never |

Controls: auto-play with pause, ←/→ step, progress dots, one caption
per scene; a caption is read over slow camera motion, not over a still
frame (decision 14). `prefers-reduced-motion` replaces flights with
cross-fades and holds the camera still for the whole scene.

## Objective

- A `/tour` route in the web UI, reached from one header link next to
  `+ New item`. It plays on arrival (decision 13).
- Zero Rust changes: items, display resolution and per-view data all
  come from existing endpoints.
- Everything lives in `ui/src/lib/tour/` plus `ui/src/routes/tour/`;
  nothing outside those folders learns about the tour except the header
  link. The tour imports from the rest of the UI (card styling, API
  client, display resolution); nothing imports from it.
- Layouts are pure functions (`ViewData → Map<id, {x, y, z}>`) with unit
  tests, the same shape as `timerMath.ts`. Graph via dagre (already in
  the tree through cytoscape-dagre, now a declared dependency), the rest
  hand-rolled.

## Out of scope

- A `tour:` section in `config.yaml` (scene order, disabling the
  link). Add only if a project asks for it.
- The CLI `render` exit and any static export of the tour.
- WebGL / Three.js. DOM cards cap out around a few hundred items; the
  scale guard below is the answer, not a renderer swap.
- The tour as a view kind. It is not in `views.yaml`, `ViewType`, or
  the add-a-view-kind checklist in `docs/architecture.md`.

## Decisions taken

Recorded 2026-09-02 after reviewing a standalone motion prototype
(47 fake cards, all nine scenes, ~350 lines, no dependencies).

1. **The scene list above, in that order.** Rejected: ending on a
   "The End" card — the last layout *is* the default view, and the
   tour navigates to it.
2. **Real DOM cards moved with CSS 3D transforms**, the camera being a
   single transform on the world container. Cards are the existing
   card look, display roles work for free, no new dependency.
   Rejected: WebGL — loses DOM reuse for a ceiling no workdown project
   is near.
3. **Zero configuration.** `views.yaml` and the `config.yaml` roles
   are the tour's configuration; the tour declares no field names of
   its own (only `id` is privileged). A missing source skips the scene.
4. **Scene data comes from the server's existing `ViewData`**
   (`BoardData`, `TreeData`, `GraphData`, `GanttData`, `MetricData`)
   for the views a scene is derived from, not from re-deriving
   grouping, filters or gantt bars in the browser. Same `where`
   clauses, same display resolution as the real views.
5. **Edges draw only after the cards have settled** into a front-on
   layout (structure, dependencies) and fade before the next
   transition. An SVG overlay inside the world; no 3D lines.
6. **Scale guard**: above a card-count threshold, only depth ≤ 1 of the
   hierarchy renders as cards, the rest as dots. Matches the goal —
   structure, not details.
7. **Starting timings from the prototype**, to be tuned later: 7 s
   fly-through, 1.6 s per reorganisation with a ~25 ms stagger, tilted
   entries (tree 28°, columns −22°) that settle to front-on.

## Decisions taken during implementation

Recorded 2026-09-02 while building on `feature/project-tour`.

8. **`views.yaml` alone, not the `config.yaml` roles.** The browser
   never sees `defaults.board_field` and friends; the tour derives each
   scene from the first view of a kind and fetches that view's data.
   This is decision 4 taken to its end: a scene is always a real view,
   so its `where` clause applies too (this repo's dependency graph
   hides done items and has no edges left, so that scene is skipped).
9. **Hand-rolled tree, not d3-hierarchy.** A tidy tree gives every leaf
   its own column and is thousands of pixels wide; siblings that are all
   leaves stack vertically under their parent instead.
10. **Tall board columns wrap into blocks** of sub-columns sized about
    3:2 (rows = ⌈√(1.5·n)⌉). A single column of 114 done items fitted
    the camera as an unreadable sliver; a block still reads as "this
    pile is the big one".
11. **The camera is fitted, not hand-posed.** Each layout's bounds are
    framed into 86% of the width and 62% of the height (room for caption
    and controls); the tilted enter pose is an offset from that fit.
12. **Title-scene cards at 35% opacity**, numbers at 18%: with 145 real
    items the cloud fought the title in a way 47 fake cards did not.
13. **The tour plays on arrival**, reversing the objective's "no
    auto-play on first visit". The tour is reached by an explicit header
    link, so opening it is already the decision to watch it, and a stage
    that waits for a second click reads as broken. Space pauses, and a
    pause freezes the tour clock rather than one tween: cards, camera
    and the scene countdown all stop together, and stepping while paused
    composes the scene at rest instead of holding it mid-flight.
14. **The camera keeps moving under a caption**, reversing "the camera
    never moves while a caption is on screen for reading". The slow ease
    from the tilted entry to the front-on hold pose is exactly what the
    caption is read over, and the 7 s fly-through is nothing but camera
    motion under a caption. The still camera survives in reduced motion,
    which enters each scene at its rest pose and stays there.

## Open questions, resolved

- `board/Card.svelte` carries drag-and-drop, the timer dot and the
  Markdown body; the tour renders its own two-line card inline (title,
  then subtitle or id) and borrows only the `.tinted` recipe.
- Scale guard threshold: 150 cards; above it, the tree's leaves are dots.
- The numbers scene shows the first metric view's rows only.

## Follow-ups

- Tune timings and camera offsets on a few real projects.
- The tour is built once for the viewport size at mount; a window
  resize mid-tour keeps the old framing until the page is reloaded.
