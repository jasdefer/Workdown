---
status: done
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

## Outcome (verified 2026-08-31)

Done, and — like [[value-coercion-layering]] — inside the
[[maintenance-review-2026-08]] PR (`03508c8`) rather than as its own
change, which is why it stayed open. The line numbers above refer to the
old combined file; `views_check.rs` is now code only (825 lines) with its
tests in a sibling `views_check/tests.rs`.

The fix took the shape this item proposed. `ViewMetricRow*` diagnostic
kinds no longer exist anywhere in the crate; a metric row is now located
by a `SlotLocus` carrying `metric_index: Option<usize>`
(`crates/core/src/model/diagnostic.rs:271`), and `check_metric_row`
(`views_check.rs:709`) calls the very same `check_aggregate_value_slot`
and `check_where_clauses` an ordinary view calls. One path, the locus
supplied by the caller — so the rule is enforced once and a row-specific
variant cannot drift from it.

The stale doc table went one better than "fix it while touching it": it
was removed rather than corrected. Which types each aggregate accepts now
lives in `view_slots::aggregate_value_types`, read by both the checker and
the web UI's create form, so the form offers exactly the fields the check
accepts and there is no prose copy left to go out of date.
