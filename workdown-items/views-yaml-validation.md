---
id: views-yaml-validation
type: issue
status: to_do
title: views.yaml JSON Schema and cross-file validation
parent: foundation
depends_on: [views-yaml-design]
---

Ship `defaults/views.schema.json` (editor autocomplete) and add cross-file validation (view → schema references and type compatibility, plus `where:` clause checking) so bad view configs are caught by `workdown validate` instead of failing at render time.

## What's already done (from `views-yaml-design`)

- `crates/core/src/model/views.rs` — typed model (`Views`, `View`, `ViewKind` with one variant per view type, `ViewType`, `Aggregate`, `Bucket`)
- `crates/core/src/parser/views.rs` — loader with serde (`deny_unknown_fields`) + per-type required-slot checks + unique-id check + 17 tests
- `docs/views.md` — design note, slot semantics, 11-view example

So "load-time structural validation" is **already done**. What's missing is everything the parser can't know without `schema.yaml`.

## Key context: ADR-005 (JSON Schema is editor-only)

`docs/adr/005-json-schema-editor-only.md` decided that `schema.schema.json` / `resources.schema.json` are **editor-only artifacts** — the CLI never reads them at runtime. All validation is Rust (serde + manual checks).

This issue's title originally said "JSON Schema and load-time validation", but those two are independent concerns per ADR-005. The JSON Schema ships for editor autocomplete only; load-time validation stays in Rust.

CLAUDE.md line ~30 still says `schema.schema.json` is "used by the CLI for validation" — that's stale and should be corrected as part of this work (see the docs item in Scope).

## What "cross-file" means

Field names in a view config are bare strings. The parser doesn't know whether `field: status` actually exists in `schema.yaml`, or whether `status` is the right *type* for a board (must be `choice`). Two checks need both files open:

- **Reference resolution** — every field name (`field`, `columns[*]`, `start`, `end`, `group`, `group_by`, `value`, `x`, `y`, `size`, `effort`, and names inside `where:` strings) must exist in the schema.
- **Type compatibility** — the slot dictates the required field type. See the matrix below.

## Scope

### 1. `defaults/views.schema.json` (editor-only)

Formal JSON Schema mirroring `schema.schema.json` / `resources.schema.json`. Discriminator-based (`type` field) with one branch per view type. Covers required/optional slots, enum values for `aggregate` and `bucket`, and rejects unknown fields. **Not loaded at runtime** (ADR-005).

When adding new view types later: update this file alongside `ViewType`/`ViewKind` (the extensibility checklist in `docs/views.md` already lists this).

### 2. Cross-file validation in Rust

New module — suggest `core::rules::views` (mirrors the existing `core::rules` pattern) — producing `Vec<Diagnostic>` from `(Views, Schema)`.

**Reference resolution:** for each field name slot, check `schema.fields.contains_key(name)`. Emit diagnostic per unknown reference carrying view id + slot name + referenced field name.

**Type compatibility matrix (errors):**

| Slot | Required type(s) |
|---|---|
| `board.field` | `choice`, `multichoice`, `string` |
| `tree.field` | `link` |
| `graph.field` | `links` |
| `table.columns[*]` | any (just check existence) |
| `gantt.start`, `gantt.end` | `date` |
| `gantt.group`, `treemap.group` | `choice`, `multichoice`, `string`, `link` |
| `workload.start`, `workload.end` | `date` |
| `workload.effort`, `treemap.size`, `metric.value`, `bar_chart.value`, `heatmap.value` | `integer`, `float` |
| `bar_chart.group_by` | `choice`, `multichoice`, `string` |
| `line_chart.x`, `line_chart.y` | `integer`, `float`, `date` |
| `heatmap.x`, `heatmap.y` | `choice`, `multichoice`, `string`, `date` |

**Bucket coupling:** if `heatmap.bucket` is set, at least one of `heatmap.x`/`y` must resolve to a `date` field. Emit an error if no date axis exists.

**Severity decision:** start with **errors only**. Add warnings later if real users report confusion. Specifically *not* doing warnings for "string used where choice was intended" — the user noted strings-as-grouping is a real, fine pattern.

### 3. `where:` clause checking

Each `where:` list entry is a string using the `parse_where` grammar. Approach:

- Call `core::query::parse::parse_where(str)`. If it fails, emit a diagnostic with the `QueryParseError` message. Do not descend further for that string.
- If it parses, walk the resulting `Predicate` tree. For each `FieldReference::Local(name)`, check the field exists in schema. Emit diagnostic per unknown ref.
- `FieldReference::Related { relation, field }` is declared in `query/types.rs` but marked "not yet supported by parser or evaluator". If/when it becomes emitted, validate the relation name is a `link`/`links` field and the target field exists. **Leave as `todo!()`-free no-op until that parser feature lands.**

### 4. New `DiagnosticKind` variants

Add variants for views issues in `core::model::diagnostic`. Suggested names (bikeshed in PR):
- `ViewParseError { path, detail }` — carries the `ViewsLoadError` message
- `ViewUnknownField { view_id, slot, field_name }`
- `ViewFieldTypeMismatch { view_id, slot, field_name, actual_type, expected }`
- `ViewWhereParseError { view_id, raw, detail }`
- `ViewBucketWithoutDateAxis { view_id }` — or fold into `ViewFieldTypeMismatch`

Each should carry the `.workdown/views.yaml` path (or the view id) so the CLI's `group_by_file` in `crates/cli/src/commands/validate.rs` can group them under one header. Extend `file_for_diagnostic` there to handle the new kinds.

### 5. Convert `ViewsLoadError` → `Diagnostic`s

Currently `parse_views` returns `Err(ViewsLoadError::{InvalidYaml, Validation})` and `load_views` adds `ReadFailed`. In `workdown validate`, don't abort on these — convert each into a diagnostic and continue (skipping cross-file checks for that file if YAML didn't parse). This matches the store's "collect all, report all" pattern.

### 6. Wire into `core::operations::validate::validate`

`operations/validate.rs:39-63` already composes three diagnostic sources (store parse + cycles + rules). Add a fourth: load `views.yaml` (path from `config.paths` — needs a `views:` field in config.yaml if not there yet; check and add if missing), convert parse errors to diagnostics, then run the cross-file checks against the loaded schema. If `views.yaml` doesn't exist, skip silently (no diagnostics).

### 7. Tests

- Unit tests per cross-file check, small fixtures (2–3 views against a minimal schema)
- Integration test against this repo's own `views.yaml` once it exists — same shape as `tests/*` for store validation
- Tests for the `ViewsLoadError` → `Diagnostic` conversion path (duplicate id, missing slot, unknown field each produce the right diagnostic kind)

### 8. Docs touch-ups

- Fix CLAUDE.md: the line saying `schema.schema.json` is "used by the CLI for validation" contradicts ADR-005. Change to "used by editors for autocomplete" and add a matching bullet for `views.schema.json`.
- Update the "Project Structure" section in CLAUDE.md to list `defaults/views.schema.json`.
- Add a short note to `docs/views.md` section "Considered but deferred" (or a new section) explaining what `workdown validate` now checks for views, so readers know where their errors will come from.

## Decisions made during scoping

- **Single entry point**: cross-file checks live inside `workdown validate`, not a separate `workdown validate views` subcommand. Matches existing UX ("one command checks everything") and fits the existing diagnostic pipeline.
- **Optional file**: `views.yaml` missing → no diagnostics, no error. Don't force users to create one.
- **Start narrow with errors**: no warnings in v1 for "works but suboptimal". Add later if real users report confusion.
- **Level B for where-clauses**: parse + walk AST to check field refs. Level A (parse only) misses typos like `typ=issue` that are exactly the kind of bug this validation is for. Level C (execute against items) gives no additional coverage.
- **No line numbers in our manual validation errors**: serde errors already carry line+column for free (malformed YAML, unknown fields). Our own errors identify by view id + slot name, which is enough to find the problem in seconds in any reasonable file.

## Open questions for the implementer

- **Default `views.yaml` in `workdown init`?** Currently `defaults/` has no `views.yaml`. Does this issue ship one (minimal: board + tree + graph)? Or leave for the render-command issue / `workdown init` to set up? Lean toward **ship it here**: a working example guides users, and without one there's nothing to render in the next issues.
- **Config file change**: does `config.yaml` need a `views:` path entry, or is `.workdown/views.yaml` hardcoded? Check `crates/core/src/model/config.rs` and decide.
- **Diagnostic variant granularity**: one catch-all `ViewConfigError` vs the five suggested variants above. Five is clearer for UX + future structured consumers (like IDE integrations), one is simpler. Go with five unless it feels ceremonial during implementation.
- **Severity of `metric.aggregate: count` + `value` set**: error (clearly user confusion) or silently ignore (value is meaningless in this case)? Leaning error so the intent is unambiguous.

## Out of scope

- Theming / styling config
- Line numbers on manual validation errors (see decisions above)
- Warnings for "loose but workable" type matches (defer)
- Multi-hop relation traversal in `where:` clauses (parser doesn't emit `FieldReference::Related` yet)
- OR nesting in `where:` — deferred at the design level
