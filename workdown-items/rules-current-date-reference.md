---
id: rules-current-date-reference
type: issue
status: to_do
title: Rules can't reference the current date
parent: code-quality
---

## Problem

Rules can express ordering between two fields on the same item (`gt_field`, `lt_field`, `gte_field`, `lte_field`, `eq_field`), but there is no way to compare a field against the current date. So constraints that combine a status with a date deadline are not expressible.

Concrete example the user wants:

> A `to_do` item must not have a `start_date` in the past — once the start date has passed without the item being picked up, that's a planning problem we want surfaced.

There's no way to write that today. The same gap blocks any rule of the form "field X relative to now" — overdue due dates on open items, stale `last_reviewed` timestamps, end_dates in the past on items still `in_progress`, etc.

`$today` exists in the codebase but only as a *generator* run at `workdown add` time — it produces a literal date stamped into the file, not a value the rule engine can resolve at validate time.

## One possible approach (not a decision)

Make `$today` resolvable as a virtual field at validation time, so the existing `*_field` operators just work:

```yaml
rules:
  - name: future-start-for-todo
    match:    { status: to_do }
    require:  { start_date: { gte_field: $today } }
    severity: warning
```

Tradeoff to think about when we get there: "today" is non-deterministic — the same files can pass on Monday and fail on Tuesday with no edits. That matches ADR-001's snapshot stance, but means CI runs on a given commit aren't reproducible without a `--as-of` override.

Other shapes worth considering at design time:
- Date-literal operands on assertions (`gte: 2026-01-01`) — orthogonal, also missing today, possibly wanted alongside.
- `$now` for timestamp fields if/when those exist.
- Whether the same mechanism should be available in `match:` conditions, not just `require:` assertions.

Decide once we pick this up.
