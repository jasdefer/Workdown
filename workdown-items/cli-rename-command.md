---
id: cli-rename-command
type: issue
status: done
title: workdown rename — change an item's id
parent: item-mutations
---

Renames a work item, which is more involved than it sounds: the id is the filename *and* what every other item links to. Carved out of `set` because it's a fundamentally different operation — file move plus cross-file reference rewrite.

```
workdown rename <old-id> <new-id> [--dry-run]
```

## Decisions

- **`id:` key in renamed file's frontmatter:** always dropped after rename. Filename carries the id; rename reconciles to filename-only. If the user wants filename ≠ id, they hand-edit afterward.
- **Default behavior:** execute immediately, `--dry-run` to preview. Consistent with `set` / `unset` / `move`; file moves are git-tracked and reversible, so a default-confirm prompt would add friction without real safety win.
- **Textual scan:** warn-only, never rewrite. Scans item bodies, `.workdown/config.yaml`, `.workdown/schema.yaml`, `.workdown/views.yaml`, `.workdown/resources.yaml`, and `.workdown/templates/*.md`. Whole-file scan for items whose parse failed. Boundaries are manual char-pair checks (not `\b`, which false-positives across hyphens — `task-1` would match inside `task-1-renamed`).
- **`old_id == new_id`:** hard error (`SameId`). Signals confusion; cheap to fail clearly.

## Architecture

Three-phase shape, mirroring `set.rs`:

1. **`preflight`** — load schema + store, validate new_id, separate `IdAlreadyExists` (store has an item with that id) from `FileAlreadyExists` (a file at the new path exists, possibly unparseable), snapshot pre-write diagnostics, enumerate referrers via `store.reverse_links` across every `FieldType::Link | Links`. **Excludes self.**
2. **`compute_plan`** — for each referrer: parse frontmatter, substitute `old_id → new_id` in link-typed fields, stage `(path, content)`. For the renamed item: same substitution (catches self-links) **plus** drop the `id:` key. Each substitution emits a `FieldRewrite { field, previous_value, new_value }` for CLI rendering. Textual scan runs here too, returns `Vec<TextualMatch>`.
3. **`execute_plan`** — referrer writes first (sorted by path for determinism). Renamed file last: write new path, then remove old path. On any I/O error return `PartialFailure { written, failed, leftover_old_path, source }` with a message naming both files. Recovery: re-run `workdown rename <old> <new>`.

## Failure model

Not atomic. Order is **referrers first, file move last** so the common failure mode (mid-referrer-write) is recoverable by re-running: already-rewritten referrers no longer match `old_id` and are skipped; the rest get rewritten; the file gets moved.

The one truly awkward case: referrers all succeed, new path written, but `remove_file(old_path)` fails (Windows AV lock, permission flap). State: both files load, store has an orphan. Surfaced explicitly in the `PartialFailure` error so the user knows to delete the leftover.

## Outcome shape

```rust
pub struct RenameOutcome {
    pub old_id: WorkItemId,
    pub new_id: WorkItemId,
    pub old_path: PathBuf,
    pub new_path: PathBuf,
    pub rewritten_files: Vec<RewrittenFile>,
    pub textual_matches: Vec<TextualMatch>,
    pub warnings: Vec<Diagnostic>,
    pub mutation_caused_warning: bool,
    pub dry_run: bool,
}

pub struct RewrittenFile {
    pub path: PathBuf,
    pub id: WorkItemId,
    pub field_rewrites: Vec<FieldRewrite>,
}

pub struct FieldRewrite {
    pub field: String,
    pub previous_value: serde_yaml::Value,
    pub new_value: serde_yaml::Value,
}

pub struct TextualMatch {
    pub path: PathBuf,
    pub line: usize,
    pub kind: TextualMatchKind,
}
```

## CLI

- `cli/mod.rs`: `Rename { old_id, new_id, --dry-run }` variant.
- `commands/rename.rs`: thin dispatcher.
- `commands/mutation_output.rs`: new `render_rename_outcome` co-located with the other per-command renderers.
- Help text mentions undo: `workdown rename <new> <old>`.

## Acceptance

- `workdown rename task-1 task-1-renamed` moves the file and rewrites every incoming link.
- Renaming to an existing id (store entry) errors with `IdAlreadyExists`; existing file at the new path errors with `FileAlreadyExists`.
- Invalid new id (uppercase, underscores, leading hyphen, etc.) errors cleanly.
- Items that had `parent: task-1` now have `parent: task-1-renamed`.
- Body of the renamed item unchanged; explicit `id:` key removed.
- A summary lists each rewritten file with field-level before/after.
- Textual mentions of the old id outside link fields are reported, not rewritten.
- `--dry-run` returns the plan without touching disk.

## Test plan (in-module unit tests)

- happy path: 1 `parent` referrer + 1 `depends_on` referrer.
- no referrers: just file move.
- user-defined link field (not parent/depends_on).
- self-link rewritten in-line with the move.
- explicit `id:` key in renamed item: dropped after rename.
- filename ≠ id case (`whatever.md` with `id: foo`): rename moves to `new.md`, drops the key.
- `IdAlreadyExists` (store collision) vs `FileAlreadyExists` (on-disk shadow): two distinct errors.
- `InvalidNewId`, `UnknownItem`, `SameId`: each returns its variant, no disk changes.
- dry-run: returns plan, no disk changes.
- body-prose: `task-1` matches inside `see task-1 above`; does NOT match inside `task-1-renamed` or `pre-task-1`.
- partial-failure recovery: mark a referrer read-only, run rename → `PartialFailure`; remove read-only, re-run → clean completion.
