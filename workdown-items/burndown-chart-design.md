---
id: burndown-chart-design
status: to_do
parent: burndown-chart
title: Decide where the burndown's time axis comes from
---

## In plain words

Before anyone draws a burndown, the project has to answer one question:
where does yesterday come from? Workdown reads the files as they are
now, and a chart of progress over time needs a record of how they were.
[[burndown-chart]] lays out three ways to get one — a date field the
user maintains, walking git history, or recorded snapshots — and this
item is where one of them gets chosen.

It commits to no answer. The output is a decision written down, the
open questions in the parent closed, and the implementation work broken
out.

## What this has to produce

- **The source of the series**, chosen from the three in
  [[burndown-chart]], with the reasoning for rejecting the others. If
  the answer turns out to be "the project picks", say what that costs.
- **An ADR if it reads history.** Git would become a dependency rather
  than a storage convention, and ADR-001 anticipated the question
  without answering it. That boundary wants recording.
- **The view kind's shape** — what the y-axis counts, how "remaining"
  is expressed without hard-coding `status == done`, whether scope
  change is drawn, how the x-axis range is set.
- **The cost estimate against the checklist** in
  [docs/architecture.md](../docs/architecture.md), which lists
  everything adding a view kind touches.
- **Follow-up items** for whatever the decision implies.

## Constraint worth holding onto

Per the generic-type rule (ADR-002), whatever ships works off field
types and view slots, not field names. No burndown may assume a field
called `status`, a value called `done`, or a field called
`completed_on`.
