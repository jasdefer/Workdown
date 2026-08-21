---
status: in_progress
title: Show which item is being timed in the views
parent: time-tracking
depends_on:
- effort-timer
---

## In plain words

While a timer runs, the item it runs on is visibly marked in every view
that presents items, so a glance at a board or a table answers "what am
I recording on right now" without opening anything. **Example:** a
board shows twelve cards; the one being timed carries the same red
recording dot the header pill does.

## Why this belongs in workdown

The pill names the timed item, but only in one place — a view full of
cards says nothing about which of them is live. The running timer is
the one piece of app state that belongs *on* the items, because it is
about exactly one of them.

## Decisions taken

1. **One symbol: the recording dot.** The same red dot the header pill
   and the timer slot already show — one mark, one meaning, learned
   once. Where a dot cannot live (a canvas, an SVG tile), the item's
   own shape takes a red edge instead: the dot's meaning in the only
   vocabulary that surface has. No mixing beyond that — an outline was
   considered as the primary mark and dropped because it fights the
   tint stripe and border that cards already carry.
2. **Per view:** board — the dot in the card's header next to the id;
   table — the dot before the first cell's link (an outline on a table
   row renders unreliably across browsers); tree — the dot beside the
   id; gantt — a red outline on the bar itself (a dot floating on a
   colored bar reads poorly, and the label column stays clean); graph —
   a red node border (Cytoscape draws to canvas, CSS cannot reach it);
   treemap — a red stroke on the item's tile. The aggregating views
   (charts, metric, heatmap, workload) present no items and get
   nothing.
3. **Client-side only.** The timer store already holds the running item
   in every tab and refetches on the timer-named live-update event;
   each view compares ids and toggles a class. View data stays
   timer-free — a running timer is not repo state, and marking an item
   must never refetch a view.
4. **No view-level banner.** A strip above the board ("recording X")
   was considered and rejected: the header pill already says exactly
   that, from every page.
5. **The red gets a name: `--color-recording`.** One token, aliased to
   the red the pill was already using, shared by the dot, the gantt
   outline, the graph border and the treemap stroke — "recording"
   stops borrowing the error color's semantic by accident.
6. **The mark is supplementary.** It carries a "Being timed" hover
   title, but the timer slot and the pill remain where the state is
   read and acted on — the views only point.

## Not in scope

- Elapsed time anywhere in a view — the pill has it.
- Any interaction on the mark itself (start, stop, open).
- Anything persisted; the mark exists only while the server holds a
  running timer.
