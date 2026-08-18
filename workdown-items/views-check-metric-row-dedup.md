---
status: to_do
parent: misc-work
title: Collapse the metric-row duplicates of the generic view checks
tags: [tech-debt]
---

## In plain words

The logic that checks views for configuration mistakes exists twice:
once for ordinary views, once for the metric rows inside stats views.
Both copies have to be kept identical by hand, and they have already
drifted apart once. **Example:** suppose we later allow durations in a
new kind of aggregation — someone must remember to change two places;
forget one, and a stats view accepts a configuration that an ordinary
chart rejects, with nobody noticing until a user hits the difference.

`views_check.rs` checks a metric row's `value` slot, predicate walk, and
`where:` operand loop with near-verbatim copies of the generic view
paths — three parallel structures (roughly lines 753–794 vs 850–897,
899–952 vs 996–1045, 820–847 vs 967–994) differing only in which
`ConfigDiagnosticKind` is constructed (`ViewMetricRow*` vs `View*`).

Every future rule change must be made twice, and the drift has already
happened once: the doc table in `check_aggregate_value_slot` lists
`sum` → integer/float and `avg/min/max` → integer/float/date, while
both code copies also allow `duration`.

## Scope

- One shared check path with the diagnostic-kind construction
  parameterized (e.g. by an `Option<metric_index>` sink), collapsing
  the ~120 duplicated lines.
- Fix the stale aggregate/type doc table while touching it.
- No behavior change; the existing tests on both paths must pass
  unchanged.
