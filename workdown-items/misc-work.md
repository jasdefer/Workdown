---
title: Miscellaneous improvements
status: in_progress
---

## In plain words

The standing bucket for small, unrelated improvements that have no
schedule: clean-ups, corner-case fixes, watch items with a trigger, and
the occasional small feature. Nothing in here is urgent and nothing
blocks a release. Each child stands on its own and can be picked up in
any order, whatever review or discussion it came out of.

This is not a feature in itself. It is a parent item that keeps
leftovers from floating at the root, so the root shows milestones and
genuinely new ideas rather than a mix of both and odds and ends.
**Example:** one child moves a piece of code to a tidier place, another
fixes how the tool handles an unusually large number, a third waits for
a project to grow big enough to need a cache.

## How it is used

- An item belongs here when it is small, self-contained, and not part
  of a milestone that is still open. Work that grows its own design
  round or several children is a milestone and lives at the root.
- The bucket never closes while it holds an open child, so it stays
  `in_progress` for as long as it is in use. That is by design; the
  parent-status rules are satisfied by it.
- Started 2026-08-03 with the follow-ups from the polish PR review;
  widened on 2026-09-09 to take unscheduled small work of any origin.
