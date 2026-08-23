---
id: metric-row-check-unification
status: to_do
title: Stop validating views and metric rows with two copies of every check
parent: maintenance-review-2026-08
---

## In plain words

Metric views contain rows, and each row can carry its own filter and
value settings — just like a view itself. Instead of writing each
validation once and applying it to both, every check was copied: one
version for the view, one nearly identical version for metric rows.
That is four duplicated functions and six mirrored error types, and
every future validation rule must be written, tested, and worded
twice. The fix is one function that takes "where am I checking — the
view itself, or row number 3?" as a parameter. While in the file, move
its 2,000-line test block into its own file so the implementation is
navigable again.

## The problem in detail

`views_check.rs` has a systematic duplication axis: every view-level
check has a hand-copied metric-row sibling differing only in the
emitted diagnostic variant and a threaded `metric_index`:

- `check_aggregate_value_slot` (`crates/core/src/views_check.rs:756-797`)
  vs `check_metric_row_value_slot` (`views_check.rs:853-900`) — about
  45 near-identical lines, including a byte-identical
  aggregate-to-allowed-types table at `views_check.rs:779-788` and
  `views_check.rs:880-889`.
- `walk_predicate` / `check_where_field_ref`
  (`views_check.rs:999-1048`) vs `walk_metric_row_predicate` /
  `check_metric_row_where_field_ref` (`views_check.rs:902-955`).
- The parse-walk-operand-check where-loop appears twice:
  `views_check.rs:823-850` and `views_check.rs:970-997`.

This blooms into the diagnostic model: six `ViewMetricRow*` variants
mirror `View*` variants (`crates/core/src/model/diagnostic.rs:384-424`),
each with its own Display arm (`diagnostic.rs:1068-1111`) and
`view_id_of` arm. Metric rows were the first "second place a check
applies"; the next one repeats the whole bloom unless the locus
becomes a parameter.

Two adjacent fixes in the same file:

- **Test module extraction.** Lines 1-1049 are implementation; lines
  1050-3110 are one `#[cfg(test)]` module. Moving the tests to
  `views_check/tests.rs` fixes the biggest-file-in-the-repo problem
  cheaply. Do **not** split the implementation per view kind — most
  `check_view` arms are five-line compositions of shared helpers, and
  a per-kind split would scatter that shared vocabulary.
- **Stale doc table.** The doc comment at `views_check.rs:747-751`
  says `sum` allows integer/float and `avg`/`min`/`max` allow
  integer/float/date; the code at `views_check.rs:781-787` includes
  `duration` in both. The rewrite of these functions is the moment to
  fix it.

## Objective

- One set of slot-check functions taking a "slot locus" parameter
  (top-level slot vs `metrics[i].slot`); the four twin functions are
  deleted.
- Folding the six mirrored diagnostic variants is desirable but
  changes the JSON diagnostic shape — acceptable pre-1.0 per ADR-007;
  decide explicitly rather than by default.
- Tests move to a submodule file; the stale doc table is corrected.

## Out of scope

- New validations, and any change to which configurations are valid.
