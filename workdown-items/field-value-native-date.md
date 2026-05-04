---
id: field-value-native-date
type: issue
status: done
title: Store FieldValue::Date as chrono::NaiveDate
parent: renderers
depends_on: []
---

Change `FieldValue::Date` from `String` to `chrono::NaiveDate` so dates are
stored as typed calendar values, consistent with how integers, floats, and
booleans are stored. Eliminates re-parsing in every downstream consumer and
replaces the hand-rolled format validator with chrono's parser.

## Motivation

Today `FieldValue::Date` holds a `String` that coerce validates to look like
`YYYY-MM-DD` (via `is_valid_date` in `store/coerce.rs:347-389`), then drops
the validation and keeps the raw text. Every downstream consumer that needs
real date semantics has to re-parse — and today gets away with lexicographic
comparison only because ISO-8601 strings happen to sort the same as dates.
Phase 04 renderers (gantt, workload, heatmap-with-bucket, line-chart with
date axis) need genuine calendar arithmetic; parsing at each call site is
noise.

All other typed field values carry native types (`Integer(i64)`,
`Float(f64)`, `Boolean(bool)`). Dates are the outlier.

## Scope

### 1. `FieldValue::Date` now holds `NaiveDate`

`crates/core/src/model/mod.rs` — change the variant from `Date(String)` to
`Date(chrono::NaiveDate)`.

### 2. Coercion uses `NaiveDate::parse_from_str`

`crates/core/src/store/coerce.rs::coerce_date` — replace `is_valid_date`
with `NaiveDate::parse_from_str(s, "%Y-%m-%d")`. Delete `is_valid_date`
and its helpers.

### 3. Downstream consumers stop re-parsing

Update each spot that reads `FieldValue::Date` to use the native value:

- `crates/core/src/query/format.rs::format_field_value` — format via
  `date.format("%Y-%m-%d").to_string()`
- `crates/core/src/query/sort.rs:158` — sort by `NaiveDate` directly
- `crates/core/src/query/eval.rs` — date comparisons in the `Date` match arm
- `crates/core/src/rules/condition.rs:110` — date equality check
- `crates/core/src/generators.rs` — `$today` returns a `NaiveDate` string
  written to YAML (still emits `YYYY-MM-DD` on disk)

### 4. Cargo feature

`crates/core/Cargo.toml` — add `serde` to chrono's feature list:

```toml
chrono = { version = "0.4", default-features = false, features = ["clock", "serde"] }
```

Lets `NaiveDate` round-trip through serde as `"YYYY-MM-DD"` without custom
code, once views/renderers need JSON output.

### 5. Tests

Update existing date tests in `store/coerce.rs` and anywhere else dates are
asserted on. Values on disk and in serde output stay `"YYYY-MM-DD"` — only
in-memory representation changes.

## Acceptance

- `FieldValue::Date(NaiveDate)` throughout the codebase; no remaining
  `Date(String)` references.
- `is_valid_date` and its helpers are deleted.
- `cargo build --workspace` and `cargo test --workspace` pass.
- Existing date tests updated to construct `NaiveDate` values directly.
- `workdown validate` and `workdown query` human output is byte-identical
  before/after — this is an internal refactor.

## Out of scope

- Adding date arithmetic APIs (days between, month diff) — comes when a
  view needs it.
- Timezone handling — dates stay naive. Revisit only if a view type
  actually needs timezone-aware datetimes.
- Changing the on-disk format — `YYYY-MM-DD` stays.
