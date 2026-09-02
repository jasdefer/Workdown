---
id: project-tour
title: Animated project tour in the web UI
status: to_do
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
| Numbers | size and health | the first `metric` view's rows; fallback: item count + count per `board_field` | never (fallback) |
| Structure | how work is organized | `defaults.tree_field` | field unset |
| Grouping | where things stand, who owns what | `defaults.board_field`; one more scene per further `board` view with a distinct field, capped at two | field unset |
| Dependencies | what blocks what; most-depended-upon item highlighted | `defaults.graph_field` | field unset |
| Timeline | when; a "today" marker | the first `gantt` view (start + end/duration already resolved by the server) | no gantt view |
| Landing | "now go explore" | the first view in `views.yaml`, the same one `/` redirects to | never |

Controls: auto-play with pause, ←/→ step, progress dots, one caption
per scene. The camera never moves while a caption is on screen for
reading. `prefers-reduced-motion` replaces flights with cross-fades.

## Objective

- A `/tour` route in the web UI, reached from one header link next to
  `+ New item`. No auto-play on first visit.
- Zero Rust changes: items, display resolution, per-view data and the
  config roles all come from existing endpoints.
- Everything lives in `ui/src/lib/tour/` plus `ui/src/routes/tour/`;
  nothing outside those folders learns about the tour except the header
  link. The tour imports from the rest of the UI (card styling, API
  client, display resolution); nothing imports from it.
- Layouts are pure functions (`items → Map<id, {x, y, z}>`) with unit
  tests, the same shape as `timerMath.ts`. Tree via d3-hierarchy, graph
  via dagre (both already dependencies), the rest hand-rolled.

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

## Open questions

- Reuse `board/Card.svelte` (drag-and-drop, click-to-open baked in)
  behind a static mode, or a slim `TourCard` sharing its styles.
  Decide on sight of how coupled it is.
- The card-count threshold for the scale guard.
- Whether the numbers scene shows every metric view's rows or the first
  view's only.
