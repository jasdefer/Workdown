---
title: Views show colors without saying what they mean
status: to_do
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
