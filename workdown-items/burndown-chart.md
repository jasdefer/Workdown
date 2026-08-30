---
id: burndown-chart
status: to_do
title: A chart that shows progress over time (burndown or similar)
---

## In plain words

Show whether the work is actually getting done: a chart with time along
the bottom and remaining items going down as they finish. Every other
tool has one, and it answers the question a board cannot — not "what is
left" but "are we closing things faster than we open them".

The catch is that workdown has no memory. Every chart it draws today is
drawn from the files as they are right now, and "right now" contains no
yesterday. A burndown needs a past, and the project has to decide where
that past comes from before anyone can draw one.

From GitHub issue #51.

## Why this is not just another view kind

The shipped chart kinds — `bar_chart`, `line_chart`, `heatmap`,
`workload`, `metric` — all read the current snapshot. A `line_chart`
can already put a date field on the x-axis, so "items by due date" is a
solved problem. What none of them can do is say what the project looked
like on a day that has passed.

ADR-001 makes validation snapshot-only and explicitly leaves the door
open: *"transition rules may be added later for visualization or CLI
command guidance, but not for validation."* So history for a chart is
allowed by the architecture — it has simply never been done, and doing
it is the actual content of this milestone.

## The three ways to get a time axis

Not a decision — the options as they stand, to be chosen between.

1. **A date field the user maintains.** A `completed_on` date, written
   when an item is finished, and the burndown counts items whose date
   is on or before each day. Costs nothing architecturally: it is a
   cumulative count over an existing `date` field, arguably a variant
   of `line_chart` rather than a new kind. Costs the user discipline —
   an unset `completed_on` is invisible, so the chart is only as honest
   as the bookkeeping, and it says nothing about items that existed
   before anyone started filling the field.

2. **Read git history.** The repo *is* the record: every status change
   is a commit, so the true series can be reconstructed by walking the
   log and re-parsing the items at each point. Nothing for the user to
   maintain, and it works retroactively over the whole project. But it
   makes the tool depend on git for the first time (today git is the
   storage convention, not a dependency), and re-parsing every item at
   every commit is expensive on a project of any age — [[project-load-cache]]
   is already watching the cost of parsing the project *once*.

3. **Recorded snapshots.** A command appends a dated tally to a file;
   the chart reads that file. Cheap, explicit, and the data is
   committed like everything else. But it only starts working the day
   someone remembers to run it, and it wants a scheduler to be useful —
   which the tool does not have and should probably not grow.

## What has to be settled

- Which source above, and whether the answer is one of them or a
  choice left to the project.
- What the y-axis counts. Item count is the obvious default and needs
  no schema support; effort or story points need an aggregate over a
  numeric field, which is a different (and more useful) chart. Whether
  both are the same view kind with a slot, per the generic-type rule,
  or two kinds.
- What "remaining" means without a name-magic status field. Boards work
  off any `choice` field, so the chart cannot hard-code `status ==
  done`; it needs a filter or a named set of terminal values.
- Whether scope is fixed or the chart shows scope change too — the
  scope line going up is usually the most interesting part of a real
  burndown, and it comes free with options 2 and 3.
- The x-axis range: earliest data to today, or a configured window.

## Notes

- Adding a view kind is a checklist, not a function —
  [docs/architecture.md](../docs/architecture.md) lists everything it
  touches (renderer, validation, web component, the create form, the
  recording dot [[recording-dot-extraction]] is about). Whatever this
  lands as pays that cost.
- If it goes the git route, it deserves an ADR: it would be the first
  place the tool reads history, and the boundary wants writing down.
