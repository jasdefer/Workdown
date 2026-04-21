---
id: views-cross-file-validation
type: issue
status: to_do
title: Cross-file validation for views.yaml
parent: foundation
---

Add the Rust cross-file validation logic that catches bad view configs at validate time instead of at render time. This issue produces the diagnostic types, the validation module, and the CLI's diagnostic-to-file mapping update. It does NOT wire validation into `workdown validate` — that's `views-validate-integration`.

## Context

`crates/core/src/parser/views.rs` already handles load-time structural validation (duplicate ids, missing required slots, unknown YAML keys via serde). What's missing: cross-file checks that need both `views.yaml` and `schema.yaml` loaded — field references must exist in the schema, and the slot dictates the required field type (e.g. `board.field` must resolve to a `choice` field).

Module placement decided: **new module `core::views_check`**, peer to `core::rules` and `core::store`. Mirrors the shape of `rules::evaluate(&store, &schema) -> Vec<Diagnostic>`, but takes `(&Views, &Schema)`.

Diagnostic style decided: **five top-level `DiagnosticKind` variants** (consistent with how `BrokenLink`, `DuplicateId`, `Cycle`, `UnknownField` are separate top-level variants, rather than the `InvalidFieldValue { detail: FieldValueError }` umbrella pattern which is reserved for cases with a shared outer shape).

## Scope

### 1. New `DiagnosticKind` variants

In `crates/core/src/model/diagnostic.rs`:

```rust
ViewParseError {
    path: PathBuf,
    detail: String,
},
ViewUnknownField {
    view_id: String,
    slot: &'static str,
    field_name: String,
},
ViewFieldTypeMismatch {
    view_id: String,
    slot: &'static str,
    field_name: String,
    actual_type: FieldType,
    expected: String,   // human-readable: "choice, multichoice, or string"
},
ViewWhereParseError {
    view_id: String,
    raw: String,
    detail: String,
},
ViewBucketWithoutDateAxis {
    view_id: String,
},
```

Add `Display` impls following the existing style (the item-level file header in CLI output already provides file context, so Display can be terse).

### 2. `core::views_check` module

New file `crates/core/src/views_check.rs`, exported from `lib.rs`. Public API:

```rust
pub fn validate(views: &Views, schema: &Schema) -> Vec<Diagnostic>
```

Checks to implement (all errors, no warnings in v1):

**Reference resolution** — for every field-name slot, check `schema.fields.contains_key(name)`. Emit `ViewUnknownField`. Slots per type:

| Slot | Source |
|---|---|
| `board.field`, `tree.field`, `graph.field` | `ViewKind::Board/Tree/Graph { field }` |
| `table.columns[*]` | `ViewKind::Table { columns }` |
| `gantt.start`, `gantt.end`, `gantt.group` | `ViewKind::Gantt { .. }` (group optional) |
| `bar_chart.group_by`, `bar_chart.value` | `ViewKind::BarChart { .. }` (value optional) |
| `line_chart.x`, `line_chart.y` | `ViewKind::LineChart { .. }` |
| `workload.start`, `workload.end`, `workload.effort` | `ViewKind::Workload { .. }` |
| `metric.value` | `ViewKind::Metric { .. }` (optional) |
| `treemap.group`, `treemap.size` | `ViewKind::Treemap { .. }` |
| `heatmap.x`, `heatmap.y`, `heatmap.value` | `ViewKind::Heatmap { .. }` (value optional) |

**Type compatibility matrix** — emit `ViewFieldTypeMismatch` on violation:

| Slot | Allowed field types |
|---|---|
| `board.field` | choice, multichoice, string |
| `tree.field` | link |
| `graph.field` | links |
| `table.columns[*]` | any (existence check only) |
| `gantt.start`, `gantt.end`, `workload.start`, `workload.end` | date |
| `gantt.group`, `treemap.group` | choice, multichoice, string, link |
| `workload.effort`, `treemap.size`, `metric.value`, `bar_chart.value`, `heatmap.value` | integer, float |
| `bar_chart.group_by` | choice, multichoice, string |
| `line_chart.x`, `line_chart.y` | integer, float, date |
| `heatmap.x`, `heatmap.y` | choice, multichoice, string, date |

**Bucket coupling** — if `heatmap.bucket.is_some()`, at least one of `heatmap.x`/`y` must resolve to a `date` field. Emit `ViewBucketWithoutDateAxis` otherwise.

**`metric.aggregate: count` + `value: Some(_)` → error.** Meaningless config; emit `ViewFieldTypeMismatch` or a dedicated variant if it reads better — leaning `ViewFieldTypeMismatch` with `expected: "(omitted — aggregate=count takes no value)"` to avoid variant sprawl. Pick what reads cleanest in the CLI output.

### 3. Where-clause checking

Also in `views_check::validate`, for each view's `where_clauses`:

- Call `crate::query::parse::parse_where(raw)`.
- On `Err(QueryParseError)`: emit `ViewWhereParseError { view_id, raw, detail: err.to_string() }` and skip AST walking for that string.
- On `Ok(Predicate)`: walk the tree (`And`/`Or`/`Not`/`Comparison`) and for each `Comparison`, check `FieldReference::Local(name)` exists in `schema.fields`. Emit `ViewUnknownField { slot: "where", field_name }` on miss.
- `FieldReference::Related { .. }`: no-op. Parser doesn't emit it yet; the type exists for future use.

### 4. `ViewsLoadError` → `Diagnostic` helper

A helper (location: probably in `views_check` or as an `impl From<ViewsLoadError>` in `parser::views`; pick what feels less awkward):

```rust
pub fn parse_errors_to_diagnostics(
    err: ViewsLoadError,
    views_path: &Path,
) -> Vec<Diagnostic>
```

- `ReadFailed(io)` → one `ViewParseError { path, detail }`
- `InvalidYaml(serde)` → one `ViewParseError { path, detail }` (serde errors already carry line+column — preserve them in `detail`)
- `Validation(errors)` → one `ViewParseError` per `ViewsValidationError`, with path and the error's Display in `detail`

This lets `operations::validate` (next issue) emit diagnostics for parse failures instead of aborting.

### 5. CLI: extend `file_for_diagnostic`

`crates/cli/src/commands/validate.rs::file_for_diagnostic` needs to handle the new kinds so they group under the views.yaml header:

- `ViewParseError { path, .. }` → `Some(path.clone())`
- Other four view variants → `Some(<views.yaml path>)`

The views.yaml path isn't on those four variants. Options:
- **(a)** Pass `config.paths.views` through `render` → `group_by_file` → `file_for_diagnostic`. Least invasive.
- **(b)** Add `views_path: PathBuf` to each of the four variants. Bloats the data; repetitive.

Lean **(a)**. Thread a `views_path: &Path` parameter.

### 6. Tests

Unit tests in `views_check.rs`, small fixtures (build `Views` and `Schema` in-code):

- Reference resolution: unknown field in every applicable slot type
- Type compatibility: one failing case per row of the matrix (pick representative slots; don't enumerate all)
- Bucket without date axis
- metric.aggregate=count + value set
- where-clause: parse error; unknown field in where; related reference (no-op until parser supports it)
- Conversion: `ViewsLoadError::InvalidYaml`, `::ReadFailed`, `::Validation` all → `ViewParseError` diagnostics with the views path

### 7. No wire-in yet

Do NOT call `views_check::validate` from `operations::validate::validate`. That's `views-validate-integration`. This issue should land a library module plus CLI diagnostic-mapping changes that unit-test green, without changing the behavior of `workdown validate`.

## Acceptance

- `cargo build --workspace` passes
- `cargo test --workspace` passes
- All new diagnostic variants have Display impls
- `views_check::validate` covered by unit tests for each check category
- `parse_errors_to_diagnostics` covered for all three `ViewsLoadError` cases
- `file_for_diagnostic` routes all five new variants

## Out of scope

- Wiring into `workdown validate` (see `views-validate-integration`)
- Integration tests against real files (see `views-validate-integration`)
- `views.yaml` loading from disk in `operations::validate` (see `views-validate-integration`)
- Warnings for "loose but workable" type matches (deferred; errors only in v1)
- Multi-hop relation traversal in where-clauses (parser doesn't emit `FieldReference::Related`)
