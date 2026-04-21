---
id: views-validate-integration
type: issue
status: to_do
title: Wire views validation into workdown validate
parent: foundation
depends_on: [views-config-path, views-cross-file-validation, foundation-cleanup]
---

Glue issue: load `views.yaml` during `workdown validate`, convert parse errors into diagnostics, run cross-file checks, and report everything through the existing diagnostic pipeline. Adds integration tests and a docs section.

## Context

By the time this issue starts:
- `views-config-path` has shipped `Paths.views` and a default `views.yaml` (so `config.paths.views` is a real field).
- `views-cross-file-validation` has shipped the five new `DiagnosticKind` variants, `views_check::validate`, and `parse_errors_to_diagnostics`.

The work here is plumbing plus tests plus docs. No new logic.

## Scope

### 1. Wire into `core::operations::validate::validate`

`crates/core/src/operations/validate.rs` currently composes three diagnostic sources: store parse, cycle detection, rule engine. Add a fourth.

Pseudocode for the addition:

```rust
let views_path = project_root.join(&config.paths.views);
if views_path.exists() {
    match parser::views::load_views(&views_path) {
        Ok(views) => {
            diagnostics.extend(views_check::validate(&views, &schema));
        }
        Err(err) => {
            diagnostics.extend(
                views_check::parse_errors_to_diagnostics(err, &views_path)
            );
        }
    }
}
// If views.yaml doesn't exist, skip silently — no diagnostics.
```

`has_errors` computation stays as-is (any `Severity::Error` diagnostic flips it).

### 2. CLI: thread views path through rendering

`crates/cli/src/commands/validate.rs::render` and `group_by_file` → `file_for_diagnostic` need the views.yaml path so the four view diagnostic kinds that don't carry the path themselves can be grouped under the views.yaml header.

- Thread `views_path: &Path` from the `validate` command through to `file_for_diagnostic` (decided in `views-cross-file-validation`: option (a) — pass as parameter).
- Confirm diagnostics group correctly: view diagnostics under `.workdown/views.yaml`, item diagnostics under the respective `.md` file.

### 3. Integration tests

New tests in the existing integration test location (wherever `operations::validate` integration tests currently live — likely `crates/core/tests/`). Each test builds a tempdir with a minimal `schema.yaml`, `views.yaml`, and one work item, then runs `operations::validate::validate` and asserts on the diagnostic set.

Test cases:
- **Valid views** — no view-related diagnostics
- **Duplicate id** — one `ViewParseError` (via `parse_errors_to_diagnostics`)
- **Missing required slot** — one `ViewParseError`
- **Unknown field reference** (e.g. `field: nonexistent` on a board) — one `ViewUnknownField`
- **Type mismatch** (e.g. `tree.field: status` where `status` is `choice`, not `link`) — one `ViewFieldTypeMismatch`
- **Invalid where clause** (e.g. `where: ["justtext"]`) — one `ViewWhereParseError`
- **Unknown field inside where** (e.g. `where: ["typo_field=x"]`) — one `ViewUnknownField` with slot `"where"`
- **Heatmap bucket without date axis** — one `ViewBucketWithoutDateAxis`
- **Missing views.yaml** — validation succeeds, no view-related diagnostics

Assert on `DiagnosticKind` variants specifically (not just error counts) so the tests remain meaningful if the pipeline composes differently later.

### 4. Smoke test against this repo's own config

Once this lands, `cargo run -- validate` in this repo should produce no view-related diagnostics (the default `views.yaml` from `views-config-path` is valid against this repo's schema). Add this as a brief manual check in the PR description — not a test.

### 5. Docs

Add a section to `docs/views.md` (title suggestion: **"Validation"** or **"What `workdown validate` checks"**):

- List the checks performed: reference resolution, type compatibility, bucket coupling, metric aggregate/value consistency, where-clause parsing and field-ref existence
- Note: `views.yaml` is optional — missing file produces no diagnostics
- Note: all view diagnostics are errors in v1 (no warnings); future warnings may be added based on user feedback
- Link to ADR-005 for the editor-only JSON Schema context

## Acceptance

- `cargo build --workspace` passes
- `cargo test --workspace` passes, including new integration tests
- `workdown validate` on this repo produces no view-related diagnostics
- `workdown validate` on a project with a broken `views.yaml` surfaces each violation grouped under `.workdown/views.yaml` in human output
- `docs/views.md` has a validation section

## Out of scope

- Creating a richer `views.yaml` for this repo beyond the default (low-priority follow-up)
- New check types beyond those already specified in `views-cross-file-validation`
- JSON output format changes (existing diagnostics serialize cleanly via the new variants' `Serialize` derive)
