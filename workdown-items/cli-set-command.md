---
id: cli-set-command
type: issue
status: done
title: workdown set — replace a field value
parent: item-mutations
---

The foundation command for item mutations. Replaces a single field on a single work item.

```
workdown set <id> <field> <value>
```

This issue covers **replace mode only** and the shared scaffolding that later mutation commands sit on top of. Type-aware modes (`--append`, `--remove`, `--delta`) are a separate issue ([[cli-set-modes]]).

## Initial idea

- Three positional arguments. No extra flags in v1.
- Looks up the item by `id`, validates the new value against the schema, writes the updated frontmatter back to the file, preserves the body byte-for-byte.
- For list/links fields: comma-separated value (`workdown set task-1 tags auth,backend`). List elements don't contain commas in practice.
- Schema violation: save anyway, warn, exit non-zero (per the milestone's save-with-warning convention).
- I/O or parse error on the target file: don't save, exit non-zero, clear message.
- Setting `field == "id"` is rejected with a pointer to [[cli-rename-command]] — renaming is a different operation (file move + reference rewrite).

## Shared scaffolding to build here

These pieces live in this issue because [[cli-unset-command]] and [[cli-set-modes]] will reuse them. Extract first, add `set` behavior on top.

- New module `crates/core/src/operations/frontmatter_io` holding the YAML writer currently inlined in `add.rs` (`build_frontmatter_yaml` and any other helpers needed by both commands).
- `add.rs` migrated to call the new module — pure code movement, no behavior change.
- `add.rs` also drops its diagnostic filter so it shows all warnings, matching the milestone's "always show all" convention. Tests adjusted.

## Core function

```rust
pub fn run_set(
    config: &Config,
    project_root: &Path,
    id: &WorkItemId,
    field: &str,
    value: serde_yaml::Value,
) -> Result<SetOutcome, SetError>;
```

`SetOutcome` shape is the one defined in [[item-mutations]]. `SetError` covers: unknown id, unknown field, `field == "id"` (with a hint), I/O failure, parse failure on the target file.

## Acceptance

- `workdown set task-1 status in_progress` writes the file and prints `task-1: status: open → in_progress`.
- An invalid choice value still saves the file but emits a warning listing allowed values, exit code non-zero.
- Unknown item id errors cleanly without touching disk.
- Unknown field name errors cleanly.
- List field value: `workdown set task-1 tags auth,backend` writes `tags: [auth, backend]`.
- Body content unchanged after a `set`.
- `set id` errors with a "use `workdown rename`" message.
- Diagnostics for other items (e.g. a chain conflict on an ancestor) are surfaced.

## Open questions to think about during implementation

- What does the CLI output look like when there are several warnings? Concise summary then details, or just a list?
- Should `previous_value: None` (field was absent) render differently than `previous_value: Some(x)`? Probably `(unset) → x` rather than `null → x`.
- For coercion: do we run the value through the same `coerce_value` path the store uses, or a slimmer "single field" variant? The store version expects a `RawWorkItem` context — may need a narrower entry point.
