---
title: Views show colors without saying what they mean
status: done
parent: misc-work
tags: [web-ui, views]
---

## In plain words

Views color their items through the `color` display role, but nothing
on the page says what a color stands for. A reader sees blue, green and
yellow cards and has to know the project's schema by heart, or open
`schema.yaml`, to learn that blue means "in progress".

This is not trivial to fix, because a color usually has no single
stated meaning:

- The color is often **derived**: a `when` list of conditions (first
  match wins) or a compute expression. The meaning of "blue" is then a
  condition such as `status == "in_progress"`, not a label anyone wrote
  down.
- A **hand-written color** on an item always wins, so one color in a
  view can come from a rule on some items and from a manual choice on
  others.
- Several conditions can produce the **same color**, and the `default`
  catches everything that matched nothing.

## Requirement

A reader of a colored view can find out what the colors mean without
leaving the view.

## Decisions taken

Taken 2026-11-27.

1. **The legend is read from the schema, not guessed from the items.**
   A color cannot be calculated: `compute` and `aggregate` are not
   valid on a color field, and no operator of the expression language
   yields a color. A color is written by hand, chosen by a `when` list
   or copied by a `pull`. A `then:` is always a literal, so a `when`
   field is a finite table from conditions to colors.
2. **Only colors on screen, with a count.** The recipe supplies the
   wording, the items the view presents supply the selection.
3. **No record of the matched branch is needed.** Because `then:` is a
   literal, a derived color names its entry by itself. Branches with
   the same color share one entry, and so does the `default`.
4. **A hand-written color is listed apart** as "set by hand". The
   origin of a value is known per item and field since
   [[item-value-origin]].
5. **Three levels by mechanism.** `when`: the full legend. `pull`: one
   line naming the link the color is inherited over, because the
   schema knows the source and not the meaning. No mechanism: nothing.
   A mechanism added later must state which level it gets (the code
   matches on the mechanisms exhaustively); until it can list its
   colors it gets a line naming the fields it reads.
6. **"Color by Status" when every condition tests the same field for
   equality.** Each color is then labelled with the values that field
   has on the items in the view, not with the branch values: gray is
   `removed` and the `default` in this repo, and a view without removed
   items reads "to_do", not "removed, other". Otherwise each condition
   is spelled out; one that is more than a single plain comparison is
   shown as written in the schema.
7. **Always visible**, as one quiet row above the view. A collapsed
   legend would not answer the question the item asks.
8. **The legend travels beside the view data**, not inside it: the
   view endpoint answers `{ view, color_legend }`. It is the same
   question for every kind that tints, so no extractor carries it.

Not part of this item:

- A `label:` on a `when` branch, so the author names the meaning.
- The reason for one item's color in the item detail (this one does
  need the matched branch).
- Clicking a legend entry to highlight or filter.
- A legend in the Markdown files of `workdown render`.

## Ideas (suggestions, not decisions)

The implementer decides all of this after thinking it through.

- A legend per view that lists the colors actually present and, where
  the color field is derived by `when`, the condition behind each one.
- Showing the reason per item instead of (or in addition to) a legend:
  a tooltip or a line in the item detail saying which condition gave
  this item its color, or that it was set by hand.
- Letting the schema author name the meaning (a label next to each
  `when` entry) so the legend shows words instead of raw conditions.
- Points to consider: what to show for hand-written colors and for
  compute expressions that are not a simple condition list, whether the
  rendered Markdown views (`workdown render`) get a legend as well, and
  how the legend stays out of the way on views with many colors.
