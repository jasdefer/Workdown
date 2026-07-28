# ADR-009: Computed fields and project constants

**Status:** Accepted
**Date:** 2026-07-27

## Context

Aggregated fields (ADR-003) derive values *across* items — same field,
rolled up the parent chain. There was no way to derive a value from
other fields of the *same* item (`end_date = start_date + duration`,
`cost = effort × rate`), and no home for project-level scalars like a
daily rate or a work-hours-per-day convention.

## Decision

- Fields declare `compute: <expression>` — arithmetic (`+ - * /`,
  parentheses, numeric literals) over the same item's fields and
  `$constants.<name>` references. Only integer, float, date, and
  duration fields participate; expressions type-check at load against a
  closed algebra (`date − date → duration`, `duration / duration →
  float`, …). No conditionals, no functions — value mappings (status →
  color) are a future declarative mechanism, not expression syntax.
- Constants are *data*, not structure: they live in a reserved
  `constants` section of `resources.yaml`, each a typed scalar coerced
  at load.
- Computed values are never stored. Like aggregates, they are derived
  into the in-memory model at load and are indistinguishable
  downstream. A hand-written frontmatter value always wins; compute
  fills only absence.
- Composition: a field with only `compute` evaluates wherever its
  inputs resolve, rolled-up inputs included (milestone flow efficiency
  is `sum / sum`). A field with both `compute` and `aggregate` computes
  on leaves only; the rollup fills everything above (a milestone's
  `end_date` is the max of its children, not its gap-blind rolled-up
  `start + duration`).
- Evaluation runs per field in topological order over compute
  references. Config-shape errors fail at schema parse; cross-file
  findings (unknown references, type errors, cycles) are diagnostics
  pinned to `schema.yaml`, so one typo disables one field, not the
  project.

## Consequences

- End dates, costs, flow efficiency, lead/cycle time become schema
  configuration instead of manual upkeep.
- The store's derive pass gained a second mechanism next to the rollup,
  orchestrated by field-dependency order.
- The expression language is deliberately minimal; growth happens by
  widening the algebra, not by adding control flow.
