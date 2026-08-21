---
id: graph-item-open
status: done
title: Clicking an item in the graph view does not open it
tags: [bug]
parent: dogfood-bugs
---

## In plain words

In the web UI, clicking a work item in a dependency graph does nothing.
Every other view that shows work items opens the item panel on click,
so the graph looks broken rather than deliberately read-only.

**Example:** a dependency graph shows twelve items and their arrows.
You spot the one that is blocking everything, click it — and nothing
happens. To read it you have to remember its title, switch to the table
view, and find it there.

## What needs to be done

- Clicking a work item in the graph view opens that item's panel, the
  same way clicking it in the board, tree, table or gantt view does.
- The treemap view has the same gap: its rectangles are work items and
  clicking them does nothing either. Fix both.
- Views that show aggregates rather than individual items (heatmap,
  workload, the charts) are out of scope — there is no single item to
  open.
