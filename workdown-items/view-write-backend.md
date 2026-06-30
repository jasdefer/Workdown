---
id: view-write-backend
type: issue
status: done
title: Persist view definitions to views.yaml
parent: view-authoring
depends_on: []
effort: "16h"
---

The serve API can read views but cannot write them. The mutation work that landed for work items writes individual item files — it does not touch `views.yaml`. So there is currently no path for the UI (or any non-editor caller) to add a view or change a view's filter and have it persist.

This issue adds the ability to persist view definitions back to `views.yaml`: creating a new view, and adjusting an existing view's `where:` filter. It is the foundation both the filter editor and the view-creation menu build on. Like every other mutation in the tool, the repo stays the source of truth — changes update the working tree only, and the user commits when they choose.

## What we want

- A new view can be added to `views.yaml` from outside a text editor.
- An existing view's `where:` filter can be changed and persisted.
- A write that would produce an invalid `views.yaml` is reported back with the same diagnostics a hand-edited file would surface — the user is never left with a silently broken file.
- Writes update files only; nothing is staged or committed automatically.
- The persisted result reads cleanly — a human opening `views.yaml` afterwards sees a sensible, hand-editable file, not machine noise.

## Acceptance

- After creating a view through this path, the view appears in `views.yaml` and renders via the existing read endpoints.
- After changing a view's filter through this path, the new `where:` is reflected in `views.yaml` and in what the view shows.
- An invalid write returns diagnostics and does not leave `views.yaml` in a broken state.

## Out of scope

- Deleting, renaming, reordering, or fully re-slotting views — text-editor job for now (revisit if a UI need surfaces).
- Auto-commit / git integration.
- A UI — this is the persistence capability; [[view-filter-editor]] and [[view-creation]] consume it.

## Design decisions

- **Whole-file re-serialize.** `views.yaml` holds every view in one file, so any write rebuilds the whole file from the in-memory `Views` model via `serde_yaml`, mirroring how item-field mutations rebuild frontmatter in `operations/frontmatter_io.rs`. Consequence: a user's hand-written comments and custom key ordering in `views.yaml` are not preserved across a write. Accepted as a known limitation for now; a follow-up can add comment preservation if real use proves it painful.
- **Filters stay strings.** A view's `where:` is already `Vec<String>` of raw clauses in the model, parsed and validated by `query::parse::parse_where`. The write path keeps them as strings end to end — the caller supplies clause text, this layer validates it with the existing parser and stores it verbatim. No `Predicate`-to-string serializer is introduced, so there is exactly one filter grammar and one validator.
- **Validate the candidate before writing.** Construct the new `Views` in memory, serialize it, then re-parse and run `views_check` on the candidate. An unparseable or structurally invalid result is rejected and the file on disk is left untouched. Semantic issues that still parse (for example a `where:` referencing a field that does not exist) are written and surfaced as diagnostics, consistent with the save-with-warning behavior in ADR-001.
- **Atomic write.** Reuse `operations::frontmatter_io::write_file_atomically` (temp file + rename) so a crash mid-write can never leave `views.yaml` half-written.

## Implementation plan

1. **Core operation — `crates/core/src/operations/view_write.rs` (new).**
   - `add_view(config, project_root, definition) -> Result<ViewMutationOutcome, ViewWriteError>` — loads the current `views.yaml`, converts the incoming definition through the existing `parser::views` raw-to-`View` conversion (so per-kind required slots and `deny_unknown_fields` are enforced identically to a hand-edited file), appends it to the `Views` list, and finalizes.
   - `set_view_filter(config, project_root, view_id, where_clauses) -> Result<ViewMutationOutcome, ViewWriteError>` — loads, finds the view by `id`, replaces its `where_clauses`, and finalizes. Unknown `view_id` is a distinct error variant.
   - Shared `finalize` helper following the existing three-phase shape: snapshot pre-write diagnostics, serialize the candidate `Views` and re-validate it in memory, write atomically only if it parses, reload, diff diagnostics to compute `mutation_caused_warning`.
   - Serializer must preserve the top-level `output_dir` and emit views in a stable order.

2. **Serialization of the `Views` model back to YAML.**
   - Add the model-to-YAML construction (the inverse of `parser::views::convert_view`), building a `serde_yaml::Mapping` per view with only the slots that view kind uses, then `serde_yaml::to_string`.
   - Add a round-trip test: serialize, re-parse, assert the resulting `Views` equals the input. This is the guard that we never emit something we cannot read back.

3. **Wire types — `crates/core/src/mutation_data.rs`.**
   - A create-view request shape mirroring the raw view fields, an update-filter request (`view_id` + `Vec<String>`), and a `ViewMutationResult` analogous to `FieldMutationResult` (id, the resulting view or filter, `mutation_caused_warning`, info messages).

4. **Server endpoints — `crates/server/src/api/views.rs` (new), wired in `api/mod.rs`.**
   - `POST /api/views` → `add_view`; `PATCH /api/views/{id}` → `set_view_filter` (replaces `where_clauses` only, matching the milestone boundary).
   - Reuse the `ApiResponse<T>` envelope and the error-to-status mapping from `api/items.rs`. An unknown view slot or malformed definition maps to `422` (clean diagnostic, not a `500`); unknown `view_id` maps to `404`; file write failure maps to `500`.

5. **Tests.**
   - Core integration tests under `crates/core/tests`: create a view and assert it appears in `views.yaml` and re-loads; change a filter and assert the new `where:` is reflected; an unparseable `where:` is rejected with the file unchanged; a semantically-off-but-parseable change writes and returns a warning.
   - Server tests exercising the two endpoints and the status-code mapping.

## Acceptance check mapping

- "View appears in `views.yaml` and renders" → step 1 `add_view` + step 5 create test.
- "Changed filter reflected in file and output" → step 1 `set_view_filter` + step 5 filter test.
- "Invalid write returns diagnostics, file not broken" → validate-the-candidate-before-writing decision + atomic write + step 5 rejection test.
