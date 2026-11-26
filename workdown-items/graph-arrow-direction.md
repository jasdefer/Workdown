---
title: Arrows in a dependency graph point against the order of work
status: to_do
parent: misc-work
tags: [views]
---

## In plain words

A graph view draws one arrow per link, from the item that holds the
field to the item it names. For `depends_on` that gives

```
pull-copy-type-conversion --> pull-copy-over-link
```

which reads "type-conversion depends on over-link". The arrow points at
the prerequisite, and in the top-down layout the item that has to wait
sits above the item that has to be done first.

That is a valid convention (it is how dependency graphs of software
packages are drawn), but it is the opposite of how a project plan is
usually read. Someone looking at a dependency chart of work expects the
arrows to follow the order of work: what comes first is at the top, and
the arrow leads to what it unblocks. Read that way the current graph
says the reverse of what is true.

The graph view is generic, which is why this has no single right
answer. It draws any `link` or `links` field, and the natural direction
depends on what the field means:

- `depends_on`: the reader expects prerequisite → dependent, the
  reverse of what is stored.
- A field such as `blocks` would be read correctly as it is drawn now.
- `related_to` has no direction at all; an arrowhead is misleading
  either way.

So simply flipping every arrow would fix `depends_on` and break other
fields.

## Requirement

A reader of a graph view can tell what an arrow means, and a dependency
graph can be read in the order the work has to happen.

## Ideas (suggestions, not decisions)

The implementer decides all of this after thinking it through.

- An option on the graph view that reverses the drawn direction, so the
  author of the view picks the reading that fits the field.
- A property on the link field in the schema that says which way it
  reads, so every view of that field agrees without repeating it.
- Using the field's `inverse` name: a graph over `dependents` instead
  of `depends_on` would draw the same edges the other way round, if a
  graph view accepted an inverse as its field.
- Saying what the arrow means instead of (or as well as) turning it: a
  label on the edges, or a line in the view's description such as
  "A → B: A depends on B".
- No arrowhead for fields without a direction.
- Points to consider: the rendered Markdown (Mermaid) and the web UI
  draw from the same edge data and should agree; the layout direction
  (top-down today) matters as much as the arrowhead for how the graph
  is read; and whether the default for an existing view may change or
  the new reading has to be opted into.
