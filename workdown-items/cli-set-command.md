---
id: cli-set-command
type: issue
status: to_do
title: workdown set — generic field mutation
parent: item-mutations
---

Add a CLI command that changes a single field on a single work item.

```
workdown set <id> <field> <value>
```

## Behavior

- Looks up the item by `id`
- Validates the new value against the schema for that field (choice membership, date parsing, link resolution, etc.)
- Writes the updated frontmatter back to the `.md` file, preserving the body
- On schema violation: save anyway, emit a warning (save-with-warning per ADR-001). Exit code non-zero so CI can catch it.
- On I/O or parse error: don't save, exit non-zero, clear error message

## Implementation notes

- Mutation logic lives in `core::set_field(...)` — a pure function taking a loaded project and returning an updated one
- The CLI is a thin wrapper: parse args, call `core::set_field`, render output
- The server will later call `core::set_field` directly — same function, same guarantees

## Acceptance

- `workdown set task-1 status in_progress` writes the file
- Invalid choice value surfaces a warning listing allowed values
- Unknown field name errors cleanly
- Unknown item ID errors cleanly
- List-typed fields: decide a syntax (comma-separated? repeated `--value`?) during implementation
