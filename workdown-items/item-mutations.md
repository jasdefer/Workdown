---
id: item-mutations
type: milestone
status: in_progress
title: Item mutations
parent: phase-04-visualization
depends_on: [foundation]
start_date: 2026-05-02
end_date: 2026-05-29
duration: "4w"
---

Add the CLI subcommands that mutate items — exercised by the UI and usable standalone. Every UI mutation maps 1:1 to a command here, and every command calls a pure function in `core` so the future server can use the same code path.

## Goals (commands in scope)

- `workdown set <id> <field> <value>` — replace a field value
- `workdown unset <id> <field>` — clear a field
- `workdown set <id> <field> --append/--remove/--delta <value>` — type-aware modes
- `workdown move <id> <value>` — shortcut for the board field
- `workdown rename <old-id> <new-id>` — change an item's id (file move + reference rewrite)
- `workdown body <id> ...` — edit the freeform Markdown body
- Audit `workdown add` for UI-driven creation

Each command has its own work item under this milestone. The text below collects the cross-cutting decisions so individual issues don't repeat them.

## Cross-cutting decisions

These are initial agreements. Implementation may sharpen them — push back in the relevant issue if you hit something that doesn't hold up.

### One verb, mode flags (vs. distinct verbs)

Frontmatter mutations use **one verb (`set`) with mode flags**, not separate verbs per operation:

```
workdown set <id> <field> <value>               # replace (default)
workdown set <id> <field> --append <value>      # for list / links / multichoice
workdown set <id> <field> --remove <value>      # for list / links / multichoice
workdown set <id> <field> --delta <value>       # for integer / float / duration / date
workdown unset <id> <field>                     # clear
```

The dispatch from `(mode, field_type)` to operation is one place to reason about validity. New modes (`--toggle`, `--prepend`, ...) land as additional flags, not new top-level commands. The shape also maps cleanly to a future `PATCH /items/:id/fields/:field` endpoint.

Body editing and renaming are separate commands because they're not field mutations — different validation, different file effects.

### Shared core layer

All mutation commands sit on top of pure functions in `crates/core/src/operations/`. The CLI is a thin wrapper that parses args and renders output. The server will call the same core functions directly.

A shared module `operations/frontmatter_io` will hold the YAML-writing helpers currently inlined in `add.rs` (`build_frontmatter_yaml` and friends). Step 0 of `cli-set-command`: lift them so `add` and `set` share one writer.

### Save-with-warning (per ADR-001)

Schema violations on mutation **save anyway**, emit a warning, and exit non-zero. Rationale: the file is the source of truth, and a hand-edit could produce the same state — the CLI shouldn't be stricter than the editor. I/O and parse errors on the target file still hard-fail without writing.

### Always show all warnings

After a mutation, surface every diagnostic from `Store::load` + `rules::evaluate`. No filter to "just this item," no flag — chain conflicts and cascade effects need to be visible at the moment the user touches that area. `workdown add` should drop its own filter for consistency.

### Whole-store load on mutation

Mutations load the full store. Needed for link resolution, sibling-id collision checks, and post-write rule diagnostics. Performance is a non-concern at workdown scale.

### Outcome shape (frontmatter mutations)

```rust
pub struct SetOutcome {
    pub path: PathBuf,
    pub previous_value: Option<serde_yaml::Value>,
    pub new_value: Option<serde_yaml::Value>,
    pub warnings: Vec<Diagnostic>,
}
```

Both values present so the CLI (and later the server) can render the change cleanly. `unset` returns `new_value: None`. `previous_value: None` means the field was absent before.

### CLI output mirrors the mode

The renderer picks the format from the mode that was requested:

| Mode | Output |
|---|---|
| replace | `task-1: status: open → in_progress` |
| append | `task-1: tags: [auth] + backend = [auth, backend]` |
| remove | `task-1: tags: [auth, backend] − auth = [backend]` |
| delta | `task-1: points: 5 + 3 = 8` |
| unset | `task-1: priority: high → (cleared)` |

### Note on naming

`render` and `serve` are also CLI commands but live in their feature milestones. This milestone collects only the commands the UI invokes as *mutations*.
