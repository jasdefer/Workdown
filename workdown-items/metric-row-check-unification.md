---
id: metric-row-check-unification
status: done
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
- The six mirrored diagnostic variants are folded into the `View*`
  family, each carrying the slot location instead.
- Tests move to a submodule file; the stale doc table is corrected.

## Decisions taken

**1. Unify both the checks and the diagnostics.** The four twin check
functions collapse into one set, *and* the six mirrored
`ViewMetricRow*` diagnostic variants collapse into the `View*` family.
Sharing only the check bodies would leave the diagnostic bloom
standing, and metric rows are the first nested locus, not the last.

**2. Location is a structured value, not a formatted string.** Each
diagnostic carries a slot location: a slot name (a compile-time
constant, as today) plus an optional metric-row index, absent for
view-level findings. Human-readable text (`metrics[3].value`) is
rendered from it, so the row index stays machine-readable for a
consumer that wants to highlight the offending row.

**3. The JSON/TypeScript diagnostic shape changes freely.** No
external consumers; the web UI reads only `scope` and the rendered
`message` and never branches on a variant name. Documentation
describes the resulting state, without changelog archaeology.

**4. One wording for both loci, house style.** Every slot-named
finding reads `view '<id>', slot '<location>': ...`, where
`<location>` is `value` or `metrics[3].value`. Where-clause findings
carry the full location too: `view '<id>', slot 'metrics[3].where',
clause '<raw>': ...`. Row messages shift slightly; view messages do
not move. Tests asserting on message text are updated here, and the
wording is then settled for [[message-style-consistency]].

**5. The doc table is wrong, the code is right.** `duration` is
allowed for `sum`/`avg`/`min`/`max` deliberately - the effort timer
writes duration values. Fix the table; do not tighten the types.
(Closes the deferral from [[stale-docs-refresh]].)

**6. `count`-with-`value` becomes one rule everywhere.** Today only
metric rows reject `aggregate: count` together with a `value` field;
`bar_chart` and `heatmap` accept it silently. The unified check
rejects it in all three. This narrows which `views.yaml` files are
valid, which is accepted: it is the same rule, enforced in one of
three places only by accident of the duplication.

**7. Test extraction lands first, as its own commit.** Move the
`#[cfg(test)]` block to `views_check/tests.rs` unchanged, so the
refactor's diff shows only real logic. Do not split the tests
thematically yet, and do not split the implementation per view kind.

## Out of scope

- New validations beyond extending `count`-with-`value` to every
  aggregate slot (decision 6).
- Splitting the implementation per view kind, and splitting the test
  file thematically.
