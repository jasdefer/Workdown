---
id: recording-dot-extraction
status: to_do
title: Extract the recording indicator the six item-presenting views each rebuilt
---

## In plain words

When the effort timer is running, the item being timed shows a
recording dot. Six view components draw that dot, and each one
implements it independently — the comparison against the running item,
the markup, the styling. Adding a seventh view kind means copying it a
seventh time, and nothing reminds you to.

## The problem in detail

The duplication sits in board `Card`, `GanttChart`, `GraphView`,
`TableView`, `TreeNode` and `TreemapView`: each compares its item
against `timerStore.runningItemId` and renders its own indicator. It
is row 14 of the adding-a-view-kind checklist in
[docs/architecture.md](../docs/architecture.md), where the "enforced
by" column reads **nothing**.

Unlike the other unguarded rows on that checklist, this one is not a
mirror of a fact written elsewhere — there is no list to compare
against, so no assertion closes it. The fix is extraction: one
component (or one small helper) the views call, so a new kind opts in
with a line instead of a copy.

The six copies are not identical today — the dot sits in different
places in different idioms (a card corner, a bar, a graph node), which
is why the extraction has a real design question in it: how much of
the placement belongs to the shared piece and how much stays with the
view. That question is why it is its own item.

## Where it came from

Split out of [[view-kind-sync-guards]], which closed the other
unguarded checklist rows. It was excluded there deliberately:
that item's tool is "assert two lists agree", and this needs a
refactor instead.

## Objective

One recording indicator, used by every item-presenting view, with the
checklist row naming it — so a new view kind adds a line rather than a
copy.
