---
id: views-config-path
type: issue
status: done
title: Add views path to config; ship default views.yaml
parent: foundation
---

Ship the `views.yaml` infrastructure: a path entry in `config.yaml`, a default file copied by `workdown init`, and the matching code change. No validation logic here — that lands in `views-cross-file-validation` and `views-validate-integration`.

## Context

`views.yaml` is the declarative config for persisted views (`workdown render`) and live bookmarks (`workdown serve`). Design is frozen in `docs/views.md`; the typed model lives in `crates/core/src/model/views.rs` and is loaded by `crates/core/src/parser/views.rs`. What's missing is the operational plumbing: where the file lives, and a starter copy for fresh projects.

Consistency goal: treat `views.yaml` the same way `resources.yaml` is treated today — a path entry in `Paths`, a default file in `crates/core/defaults/`, a description in CLAUDE.md.

## Scope

### 1. Config model

- `crates/core/src/model/config.rs` — add `pub views: PathBuf` to `Paths` as a required field (same treatment as `work_items`, `templates`, `resources`).
- Breaking change for any existing consumer config.yaml. Acceptable: no external users yet.

### 2. Default files

- Update `crates/core/defaults/config.yaml`: add `views: .workdown/views.yaml` under `paths:`.
- Create `crates/core/defaults/views.yaml` with three minimal views that mirror `defaults.board_field` / `tree_field` / `graph_field`:
  ```yaml
  views:
    - id: status-board
      type: board
      field: status

    - id: hierarchy
      type: tree
      field: parent

    - id: dependencies
      type: graph
      field: depends_on
  ```
- Update this repo's own `.workdown/config.yaml` to include the new `views:` entry so the project stays loadable.
- (Optional but nice) Create this repo's own `.workdown/views.yaml` matching the default — deferred to a follow-up if it bloats the issue.

### 3. `workdown init`

`crates/core/src/operations/init.rs` uses explicit `include_str!` constants plus per-file `write_file` calls — it does NOT auto-copy the `defaults/` directory. Add the new file explicitly:

- Add a new constant alongside the existing ones:
  ```rust
  const DEFAULT_VIEWS: &str = include_str!("../../defaults/views.yaml");
  ```
- Add a matching `write_file` call in `run_init` that writes to `.workdown/views.yaml`.
- Extend the existing init integration tests (`crates/core/tests/init.rs`) to assert the new file is created with the expected content.

### 4. Docs

- CLAUDE.md **Project Structure** block is stale: still says `src/` and `defaults/` at repo root. Update to reflect the workspace split (`crates/core/src/`, `crates/core/defaults/`). Add `views.yaml` under the defaults listing.
- CLAUDE.md **`.workdown/`** block — add `views.yaml` entry.
- CLAUDE.md **Configuration Files** section — add a one-line description for `views.yaml` (user-editable; declares persisted views rendered by `workdown render`).

## Acceptance

- `cargo build --workspace` passes
- `cargo test --workspace` passes
- `workdown init` on a fresh temp dir produces `.workdown/views.yaml` with the three default views
- `Config` deserializes the updated `defaults/config.yaml` without error
- This repo's `workdown validate` still runs (requires the repo's own `.workdown/config.yaml` to be updated)

## Out of scope

- JSON Schema for `views.yaml` (see `views-json-schema`)
- Any validation logic for views (see `views-cross-file-validation`)
- Wiring views validation into `workdown validate` (see `views-validate-integration`)
- Creating this repo's own rich `views.yaml` beyond the default starter
