---
id: duration-comparison-rule
type: issue
status: to_do
title: Cross-field comparison rule for duration values
parent: time-tracking
---

The schema rule engine has no way to compare two duration fields and
assert an ordering between them. This is the load-bearing primitive
needed to track effort separately from calendar duration: with this in
place, a user can declare an `effort` field and an `effort <= duration`
rule themselves — without it, the convention can't be enforced anywhere.

The objective is to make the rule expressible. Comparing two duration
values is well-defined when units match; when units differ (e.g. `3h`
vs `1w`), the comparison needs a project-level convention for how many
work hours fit in a calendar day.

## Open questions

- Where does the work-hours-per-calendar-day convention live — config,
  schema, per-rule, somewhere else?
- Does the rule engine compare fields generally (any two fields of the
  same type), or just durations? Generic is more useful but bigger.
- What does it do when one field is unset on a given item — skip, fail,
  configurable?
- How does a failed rule surface — warning per ADR-001, or stricter for
  this kind of check?
- Does the bare-number problem matter here (`effort: 3` — hours? days?)
  or do we sidestep by requiring units?

## Bonus / dogfooding once shipped

- Declare the convention `duration = calendar time` somewhere visible
  (it's implicit today — gantt modes assume it).
- Consider whether `effort` and `hours_per_workday` belong in
  `defaults/schema.yaml` and `defaults/config.yaml` so new projects get
  them out of the box.
- Set an `effort` value on a few leaf items in this repo to exercise
  the rule.
