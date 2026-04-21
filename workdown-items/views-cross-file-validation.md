---
id: views-cross-file-validation
type: issue
status: done
title: Cross-file validation for views.yaml
parent: foundation
---

Add the Rust cross-file validation logic that catches bad view configs at validate time instead of at render time. This issue produces the diagnostic types, the validation module, and the CLI's diagnostic-to-file mapping update. It does NOT wire validation into `workdown validate` — that's `views-validate-integration`.

## Context

`crates/core/src/parser/views.rs` already handles load-time structural validation (duplicate ids, missing required slots, unknown YAML keys via serde). What's missing: cross-file checks that need both `views.yaml` and `schema.yaml` loaded — field references must exist in the schema, and the slot dictates the required field type (e.g. `board.field` must resolve to a `choice` field).

Module placement decided: **new module `core::views_check`**, peer to `core::rules` and `core::store`. Mirrors the shape of `rules::evaluate(&store, &schema) -> Vec<Diagnostic>`, but takes `(&Views, &Schema)`.

Diagnostic style decided: **eight top-level `DiagnosticKind` variants** (consistent with how `BrokenLink`, `DuplicateId`, `Cycle`, `UnknownField` are separate top-level variants, rather than the `InvalidFieldValue { detail: FieldValueError }` umbrella pattern which is reserved for cases with a shared outer shape). Each parse-time and check-time condition that the UI may want to highlight gets its own variant so server consumers can route structured errors.

## Scope

### 1. New `DiagnosticKind` variants

In `crates/core/src/model/diagnostic.rs`:

```rust
// Parse-time (produced by parse_errors_to_diagnostics from ViewsLoadError)
ViewParseError {
    path: PathBuf,
    detail: String,
},
ViewDuplicateId {
    view_id: String,
},
ViewMissingSlot {
    view_id: String,
    view_type: ViewType,
    slot: &'static str,
},

// Check-time (produced by views_check::validate)
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
ViewCountAggregateWithValue {
    view_id: String,
},
```

Add `Display` impls following the existing style: terse form when the item-level file header provides context, path-qualified for `ViewParseError` so it reads cleanly in both grouped and ungrouped modes (matches `FileError`).

### 2. `core::views_check` module

New file `crates/core/src/views_check.rs`, exported from `lib.rs`. Public API:

```rust
pub fn validate(views: &Views, schema: &Schema) -> Vec<Diagnostic>
```

Checks to implement (all errors, no warnings in v1):

**Reference resolution** — for every field-name slot, check `schema.fields.contains_key(name)`. Emit `ViewUnknownField`. Exception: the name `"id"` is always valid without needing to be in `schema.fields` (hybrid-ID rule — it's a virtual always-present field). Slots per type:

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

**`metric.aggregate: count` + `value: Some(_)` → error.** Emit `ViewCountAggregateWithValue { view_id }` (dedicated variant — reusing `ViewFieldTypeMismatch` forces a phantom `actual_type`, which is misleading and undefined if the value field doesn't exist). The existence- and type-check on the `value` field still runs regardless: a user who writes `value: nonexistent` with `aggregate: count` gets both diagnostics, since they're orthogonal problems.

### 3. Where-clause checking

Also in `views_check::validate`, for each view's `where_clauses`:

- Call `crate::query::parse::parse_where(raw)`.
- On `Err(QueryParseError)`: emit `ViewWhereParseError { view_id, raw, detail: err.to_string() }` and skip AST walking for that string.
- On `Ok(Predicate)`: walk the tree (`And`/`Or`/`Not`/`Comparison`) and for each `Comparison`, check the `FieldReference`:
  - `FieldReference::Local(name)`: must exist in `schema.fields` (or be `"id"`). Emit `ViewUnknownField { slot: "where", field_name }` on miss.
  - `FieldReference::Related { relation, .. }`: `relation` must be either a forward link/links field name (`schema.fields.contains_key(relation)` where the field is a `Link` or `Links`) or an inverse name (`schema.inverse_table.contains_key(relation)`). Emit `ViewUnknownField { slot: "where", field_name: relation }` on miss. The `field` side of the dot is not validated here (one-hop target resolution is the runtime resolver's job; v1 keeps validation scoped to what's syntactically part of this file).

### 4. `ViewsLoadError` → `Diagnostic` helper

A helper in `views_check` (keeps `parser::views` free of `Diagnostic` knowledge — the parser stays a pure loader):

```rust
pub fn parse_errors_to_diagnostics(
    err: ViewsLoadError,
    views_path: &Path,
) -> Vec<Diagnostic>
```

- `ReadFailed(io)` → one `ViewParseError { path, detail }`
- `InvalidYaml(serde)` → one `ViewParseError { path, detail }` (serde errors already carry line+column — preserve them in `detail`)
- `Validation(errors)` → one diagnostic per `ViewsValidationError`, mapped to the structured variant:
  - `ViewsValidationError::DuplicateId { id }` → `ViewDuplicateId { view_id: id }`
  - `ViewsValidationError::MissingSlot { id, view_type, slot }` → `ViewMissingSlot { view_id: id, view_type, slot }`

Separate structured variants (rather than collapsing both into `ViewParseError`) so the live server UI can highlight which view and which slot is wrong.

This lets `operations::validate` (next issue) emit diagnostics for parse failures instead of aborting.

### 5. CLI: extend `file_for_diagnostic`

`crates/cli/src/commands/validate.rs::file_for_diagnostic` needs to handle the new kinds so they group under the views.yaml header:

- `ViewParseError { path, .. }` → `Some(path.clone())`
- Other seven view variants (`ViewDuplicateId`, `ViewMissingSlot`, `ViewUnknownField`, `ViewFieldTypeMismatch`, `ViewWhereParseError`, `ViewBucketWithoutDateAxis`, `ViewCountAggregateWithValue`) → `Some(project_root.join(&config.paths.views))`

Thread `config: &Config` and `project_root: &Path` (not a bare `views_path: &Path`) through `render` → `group_by_file` → `file_for_diagnostic`. Rationale: this same plumbing will want to route `schema.yaml` / `resources.yaml` diagnostics later, and deriving paths from `config` means future additions don't require threading another parameter each time. Symmetric with `operations::validate::validate(&Config, &Path)`.

### 6. Tests

Unit tests in `views_check.rs`, small fixtures (build `Views` and `Schema` in-code):

- Reference resolution: unknown field in every applicable slot type; `id` accepted as a `table.columns[*]` entry even when absent from `schema.fields`
- Type compatibility: one failing case per row of the matrix (pick representative slots; don't enumerate all)
- Bucket without date axis
- metric.aggregate=count + value set → `ViewCountAggregateWithValue`; combined with a nonexistent `value` field → two diagnostics (count-with-value AND unknown field)
- where-clause: parse error; unknown local field; unknown relation; valid forward relation (`parent.status`); valid inverse relation (`children.status`)
- Conversion:
  - `ViewsLoadError::ReadFailed` → one `ViewParseError`
  - `ViewsLoadError::InvalidYaml` → one `ViewParseError`
  - `ViewsLoadError::Validation` with duplicates → `ViewDuplicateId` diagnostics
  - `ViewsLoadError::Validation` with missing slots → `ViewMissingSlot` diagnostics

### 7. No wire-in yet

Do NOT call `views_check::validate` from `operations::validate::validate`. That's `views-validate-integration`. This issue should land a library module plus CLI diagnostic-mapping changes that unit-test green, without changing the behavior of `workdown validate`.

### 8. Documentation

- Rustdoc on the new `views_check` module (module-level doc stating the post-validate invariant: "after `views_check::validate` passes, every field name referenced by `views.yaml` is either in `schema.fields`, a recognized relation name, or `id`"), on `validate`, on `parse_errors_to_diagnostics`, and on each new `DiagnosticKind` variant.
- `docs/views.md` updates: one section listing the cross-file checks this issue adds, and a note in the "Filters — `where:`" section that field references are validated (forward + inverse relation names accepted).

## Acceptance

- `cargo build --workspace` passes
- `cargo test --workspace` passes
- All eight new diagnostic variants have Display impls
- `views_check::validate` covered by unit tests for each check category
- `parse_errors_to_diagnostics` covered for all three `ViewsLoadError` cases, including the split to `ViewDuplicateId` and `ViewMissingSlot`
- `file_for_diagnostic` routes all eight new variants
- Rustdoc stating the post-validate invariant on `views_check::validate`
- `docs/views.md` has a section listing the new cross-file checks

## Out of scope

- Wiring into `workdown validate` (see `views-validate-integration`)
- Integration tests against real files (see `views-validate-integration`)
- `views.yaml` loading from disk in `operations::validate` (see `views-validate-integration`)
- Warnings for "loose but workable" type matches (deferred; errors only in v1)
- Multi-hop relation traversal in where-clauses (parser doesn't emit `FieldReference::Related`)
