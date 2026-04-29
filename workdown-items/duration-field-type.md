---
id: duration-field-type
type: issue
status: done
title: Add `duration` field type
parent: renderers
---

Introduce `duration` as the 11th built-in field type, for time-quantity
values. General-purpose: not just for Gantt — any field that tracks a
length of time. Required by `gantt-duration-mode`; `workload`'s `effort`
slot is a likely future migration too.

The 10 existing types (`string`, `choice`, `multichoice`, `integer`,
`float`, `date`, `boolean`, `list`, `link`, `links`) live in
`core::model::schema::FieldType` / `FieldTypeConfig`; this adds one
variant alongside.

## Design

**Internal storage:** `i64` canonical seconds. Signed (negative
durations valid by default; opt out per-field via `min: "0s"`). Range
~292 billion years — overflow not a practical concern.

**Input grammar (one form, used everywhere):** suffix-shorthand string.

- Single component: `<integer><suffix>` — e.g. `5d`, `30s`, `120min`,
  `4h`, `2w`
- Compound: whitespace-separated single components — e.g.
  `1w 2d 3h`, `4h 30min`
- Negative: leading `-` applies to the whole expression — e.g. `-2d`.
  `-1w 2d` parses as −(1w + 2d) = −9 days.
- Allowed suffixes (exact spellings): `s`, `min`, `h`, `d`, `w`
- **No `m` accepted** — `min` is the only minutes suffix, deliberately
  chosen to avoid month-vs-minute ambiguity that other tools suffer
  from.
- Integer values only (no fractions like `1.5h`)
- No duplicate units in one expression (`1w 2w` → error)
- Empty string → error
- Component values may not individually be negative (only the whole
  expression negates): `1w -2d` → error

**Same grammar everywhere:**
- Work item frontmatter: `estimate: "5d"`, `estimate: "1w 2d 3h"`
- Schema `min` / `max` bounds: `min: "0s"`, `max: "4w"`
- CLI query predicates: `--where 'estimate > 1h'`
- Rendered display output (and `query --json` string output)

Always quote duration strings in YAML (`"5d"`) — defensive against
plain-scalar parser edge cases.

**Output (renderers and JSON):** same suffix grammar, compound,
largest-first decomposition, skip zero components, leading `-` for
negatives. Floor unit = smallest unit actually present. `0` canonical
renders as `"0s"`. Round-trip stable: rendered strings are valid input.

**Schema declaration (in `schema.yaml`):**

```yaml
fields:
  estimate:
    type: duration
    min: "0s"          # optional; same grammar as values
    max: "4w"          # optional
    aggregate:
      function: sum    # sum/min/max/average/median/count valid
```

`FieldTypeConfig::Duration { min: Option<i64>, max: Option<i64> }`
carries the pre-parsed canonical-second bounds.

## Pieces

- `FieldType::Duration` and `FieldTypeConfig::Duration { min, max }` in
  `core::model::schema`.
- New module `core::model::duration` with `parse_duration`,
  `format_duration_seconds`, and `ParseDurationError`. Hand-rolled
  parser (~80 LOC) — no library dependency. `format_duration_seconds`
  uses i128 arithmetic internally to handle `i64::MIN` without panic.
- `FieldValue::Duration(i64)` with custom `serialize_with` that emits
  the formatted string for JSON output (matches `Date`'s convention of
  serializing as a readable string).
- New `FieldValueError::OutOfRangeDuration { value, min, max }` variant
  carrying pre-formatted display strings.
- Coerce path in `core::store::coerce` reads YAML scalar string →
  `parse_duration` → applies min/max bounds.
- All 11 dispatch sites updated (some keyed by `FieldType`, some by
  `FieldValue`): query format/sort/eval, rules condition/assertion,
  store rollup, cli table renderer.
- `eval_duration` in `core::query::eval` mirrors `eval_integer`,
  parsing the RHS via the same `parse_duration` so CLI predicates work
  against duration fields.
- `RawFieldDefinition.min`/`.max` widen from `Option<f64>` to
  `Option<serde_yaml::Value>` so they can carry numbers (integer/float)
  or strings (duration), parsed type-aware in the validation pass.
- Schema-level rollup helpers in `store::rollup` extended for Duration:
  `sum`, `min`, `max`, `average` (truncates via integer division),
  `median`, `count`.
- View-time aggregate compatibility (`views_check.rs:457`): Duration
  added to allowed types for `Sum`/`Avg`/`Min`/`Max`. Charts
  (bar_chart, metric, heatmap, line_chart, treemap) numerically plot
  durations as canonical seconds via `as_number`.
- `defaults/schema.schema.json`: add `duration` to the type enum,
  permit string min/max for duration via per-type if/then.

## Acceptance

- `schema.yaml` accepts `type: duration` with optional `min`, `max`,
  `aggregate`.
- Parses, stores, and round-trips through frontmatter input → store →
  render output. Rendered strings re-parse to the same canonical
  value.
- Existing 10 field types unaffected (regression-tested).
- Unit tests for `parse_duration`: each suffix, compound, negative,
  duplicate-unit rejection, empty rejection, component-negative
  rejection, unknown-suffix rejection (especially bare `m`),
  whitespace tolerance, overflow at `i64::MAX`, round-trip property.
- `serde_yaml` sanity test: `5d`, `1w 2d`, `-2d`, `"5d"` all
  deserialize as `Value::String`.
- `workdown query` `--where 'estimate > 1h'` filters duration fields
  correctly.
- `workdown query --json` emits duration values as quoted strings
  (`"5d"`).
- Table renderer displays compound form (`1w 2d 3h`).
- Aggregate rollups produce correct totals with `sum`, `min`, `max`,
  `average`, `median`, `count`.

## Out of scope (rejected during design)

- Composite YAML mapping input (`{ days: 5 }`) — single-grammar
  decision.
- Bare integer input (`estimate: 5`) — rejected as too "magic" without
  schema lookup.
- HH:MM input — doesn't extend to days/weeks.
- ISO 8601 input (`PT5H`) — formal but ergonomically hostile;
  M-overload between minutes/months is a footgun.
- `format: compact` opt-in (always-render-in-field's-unit). Output is
  always compound; revisit if a real case needs it.
- Sub-second units (milliseconds, microseconds, nanoseconds).
- Months/years (variable length — every standard library splits these
  out as a separate type).
- Per-field restricted unit sets (e.g. `allowed_units: [hours]`).
- Locale-specific suffix aliases (`day`, `days`, `week`).
- Migrating `workload`'s `effort` slot to `duration` — separate issue
  tied to `render-workload`.
