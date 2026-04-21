---
id: foundation-cleanup
type: issue
status: to_do
title: Consolidate duplication and tighten types before more foundation work
parent: foundation
---

Address maintainability findings from the 2026-04-21 code review before landing `views-validate-integration`. Tightly scoped — items that aren't worth fixing now are captured under "Considered but deferred" so they aren't lost.

## Context

With `views_check` landed, the codebase has two nearly-identical "is this a relation anchor" checks, two structurally identical parse-error diagnostic variants, and a stringified error inside `ValidateError` that drops structure the server will want later. None of this is broken — but it'll get harder to fix once renderers and the server start consuming the diagnostic pipeline.

## Scope — fix now

### 1. Consolidate the "is relation anchor" check

Same rule — "resolves to a `link`/`links` field, or is a known inverse name" — implemented in three places:

- `crates/core/src/parser/schema.rs:691-705` — dot-notation branch of `validate_field_reference`
- `crates/core/src/parser/schema.rs:720-722` — `is_defined_inverse`
- `crates/core/src/views_check.rs:441-446` — `is_valid_relation`

Extract a single helper both sites call.

Design note: the schema parser validates *before* `Schema::inverse_table` is built (`parser/schema.rs:56`), so the helper likely lives as a free function over `IndexMap<String, FieldDefinition>` rather than a method on `Schema`. `views_check` has a fully built `Schema` available and can keep using `inverse_table` directly — or call the shared helper for symmetry. Decide during implementation.

### 2. Preserve structured error in `ValidateError::SchemaLoad`

`crates/core/src/operations/validate.rs:27`:

```rust
#[error("failed to load schema: {0}")]
SchemaLoad(String),
```

Change to `SchemaLoad(#[from] SchemaLoadError)`. Callers that want the string keep calling `.to_string()`; the server (and future tests) can pattern-match on the inner variant. Any other lossy stringifications of per-module load errors found along the way should get the same treatment.

### 3. Merge `FileError` and `ViewParseError` into one variant

`crates/core/src/model/diagnostic.rs:35` and `:89` are structurally identical:

```rust
FileError      { path: PathBuf, detail: String }
ViewParseError { path: PathBuf, detail: String }
```

Their `Display` impls are byte-identical (`diagnostic.rs:179-181` and `:241-243`). The only difference is the variant tag in JSON output; routing consumers already have `path` to distinguish work-item `.md` files from `views.yaml`.

- Delete `ViewParseError`
- Update `views_check::parse_errors_to_diagnostics` to emit `FileError`
- Update `file_for_diagnostic` in `crates/cli/src/commands/validate.rs` (same handling — `Some(path.clone())`)
- Update tests in `views_check.rs` asserting on `ViewParseError`
- Loosen the `FileError` rustdoc to cover config files, not just work items

Doing this now also sets the precedent: when schema/resource cross-file checks land, parse failures reuse `FileError` rather than spawning a third and fourth near-identical variant.

### 4. Align naming: `rules::evaluate` vs `views_check::validate`

Both produce `Vec<Diagnostic>` from `(artifact, &Schema)`. Pick one verb so a third producer lands on a clear precedent.

Suggested: rename `views_check::validate` → `views_check::evaluate`. Updates needed in `views_check.rs`, the caller in `operations/validate.rs` (once `views-validate-integration` wires it in — may be easier to rename first so that issue uses the new name), and any doc references in `docs/views.md` / issue files.

## Considered but deferred

- **`DiagnosticKind` enum size / nesting** — 17 variants, 8 view-specific. Nesting into `DiagnosticKind::View(ViewDiagnostic)` would partition responsibility cleanly but breaks the flat serde tags consumed by JSON output. Revisit before renderers start adding their own variants.
- **`views_check.rs` file size** — 951 lines, largest top-level file. Acceptable now; promote to a directory (`views_check/slot.rs`, `views_check/where_clause.rs`, `views_check/tests.rs`) when warnings or new view types push it past ~1200.
- **`check_view` as a table-driven matrix** — the 11-arm match hardcodes the type-compatibility matrix inline. Self-documenting and each case is readable; skip the refactor until adding a 12th view type gets painful.
- **Dot-notation reference validation in two input shapes** — `schema::validate_field_reference` on `&str`, `views_check::check_where_field_ref` on typed `FieldReference`. Unifying requires a common representation; one caller each justifies the duplication today.
- **Parameter threading in `commands/validate.rs::render`** — `config` + `project_root` are threaded through three layers so view diagnostics can resolve to `config.paths.views`. Explicit design choice in `views-cross-file-validation`; accept.

## Acceptance

- One shared helper for the relation-anchor check, called from both schema parser and `views_check`; no duplicated implementation
- `ValidateError::SchemaLoad` (and any similar lossy stringifications) use `#[from]`
- `DiagnosticKind::ViewParseError` removed; `views_check::parse_errors_to_diagnostics` emits `FileError`; CLI diagnostic-to-file mapping simplified accordingly
- `rules::evaluate` and `views_check::evaluate` share a verb
- `cargo build --workspace` and `cargo test --workspace` pass
- Human output of `workdown validate` is byte-identical before/after (this is cleanup, not a behavior change)

## Out of scope

- Anything listed under "Considered but deferred"
- New validation rules, diagnostic variants, or features
- Wiring `views_check` into `workdown validate` — that's `views-validate-integration`
