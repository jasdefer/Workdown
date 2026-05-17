---
id: cli-add-audit
type: issue
status: done
title: Audit workdown add for UI-driven creation
parent: item-mutations
---

Confirm `workdown add` supports everything the UI will need to create items from a browser form.

## Checks

- All schema field types settable via flags (string, choice, multichoice, integer, float, date, duration, boolean, list, link, links)
- Default generators (`$today`, `$uuid`, `$filename`, `$filename_pretty`, `$max_plus_one`) behave correctly
- Templates usable to pre-fill an add call (`--template <name>`)
- Clear error surface when a required field is missing
- `add` exposes a programmatic function in `core` that takes a struct (not just argv), so the server can build one from a JSON body

## Deliverable

Either "nothing to do, already works" documented here, or a list of specific gaps filed as sub-issues.

## Findings

The real consumer of this surface is the server (not clap) — the UI will POST JSON, the server will call `run_add` directly, the CLI flags are an existing side-effect. Findings are framed against that.

### Check 1 — Field types: **pass**

All 11 schema types (string, choice, multichoice, integer, float, date, duration, boolean, list, link, links) build a typed clap argument in `crates/cli/src/cli/schema_args.rs`, with per-type tests in the same file. The same field map shape (`HashMap<String, serde_yaml::Value>`) is what `run_add` accepts, so a server taking a JSON body lands in the same coercion path.

### Check 2 — Default generators: **pass**

`$today`, `$uuid`, `$filename`, `$filename_pretty`, `$max_plus_one` all implemented in `crates/core/src/generators.rs`, applied in two passes (slug-independent first, then `$filename*` once the slug is derived). Tests cover each.

Out of scope but worth flagging for a future audit: `$today` uses local server timezone, and `$max_plus_one` races under concurrent creates. Neither blocks a single-user UI MVP.

### Check 3 — Templates: **pass**

`--template <name>` is a fixed flag built outside the schema-driven loop in `schema_args.rs`. Precedence is CLI > template > schema defaults (documented in `operations/add.rs`). Schema-field collision is handled by yielding to the schema.

A "list templates with metadata" API for the UI form is **out of scope** — it's a discovery concern, not a creation concern. Belongs in the server milestone.

### Check 4 — Error surface for missing/invalid fields: **pass**

`Diagnostic` carries structured per-field context, not just human-readable strings:

- `ItemDiagnosticKind::MissingRequired { field: String }`
- `ItemDiagnosticKind::InvalidFieldValue { field, detail: FieldValueError }`

Both serialize as tagged JSON via serde. The UI can map an error back to a specific form field without parsing the message.

### Check 5 — Programmatic core API: **pass with two gaps, resolved inline**

`run_add(config, project_root, field_values, template) -> Result<AddOutcome, AddError>` was already server-shaped. Two adjustments landed under this audit:

- **Gap A (resolved):** `AddOutcome` now exposes `id: WorkItemId` alongside `path`, so the server doesn't have to re-derive the id from the filesystem path.
- **Gap C (resolved):** `add` previously *blocked* on schema coercion errors via `AddError::ValidationFailed`, which was inconsistent with the milestone's save-with-warning policy (ADR-001). The variant is removed; coercion errors now flow through `outcome.warnings` and flip `mutation_caused_warning: bool` (same pattern as `set`/`rename`). Hard-fails remain for I/O errors, duplicate ids, missing filename source, invalid id format, and template load failures.

The associated CLI test was inverted: `add_writes_with_warning_on_invalid_choice_value` confirms the file is created, the diagnostic appears with a structured `field` reference, and `mutation_caused_warning` is set.

### Gap B (intentional, no change)

`run_add` accepts aggregate-configured fields in `field_values` without special-casing them. This is correct, not a gap: aggregate fields are meant to be set manually on leaf items, with rollup filling in ancestors. Two ancestors in the same chain both setting it manually is the actual error case, and `AggregateChainConflict` catches it at the right level (a relational check, not a per-field reject).

The UI's responsibility is to not expose aggregate fields as editable inputs for items that have descendants. The API stays permissive.

### Out of scope (noted for later)

- Server-side JSON → `serde_yaml::Value` coercion edge cases (number vs string for integer fields, array shape for list/links). Worth a focused test in the server milestone; not an `add` concern.
- Three-way duplication of `post_diagnostics_introduced_by_mutation` across `add.rs` / `set.rs` / `rename.rs`. Cosmetic; a future shared-helpers pass can dedupe.
