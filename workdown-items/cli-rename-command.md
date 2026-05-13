---
id: cli-rename-command
type: issue
status: to_do
title: workdown rename — change an item's id
parent: item-mutations
---

Renames a work item, which is more involved than it sounds: the id is the filename *and* what every other item links to. Carved out of `set` because it's a fundamentally different operation — file move plus cross-file reference rewrite.

```
workdown rename <old-id> <new-id>
```

## Initial idea

- Validate `<new-id>` against `is_valid_id` (kebab-case rules) and check it isn't already used by another item.
- Move the file: `workdown-items/<old-id>.md` → `workdown-items/<new-id>.md`.
- If frontmatter has an explicit `id:` key, update it (or remove it if the new id matches the filename).
- Walk every other work item, find every `link`/`links` field that points to `<old-id>`, rewrite it to `<new-id>`. This covers built-in relations (`parent`, `depends_on`, `related_to`, `duplicates`) and any user-defined link field — driven generically by `FieldType::Link`/`Links`.
- Surface a summary: which files were rewritten, how many references touched.

## Atomicity

The hard part. We're touching N files. Options sketched here; pick during implementation:

1. **Best-effort sequential.** Write each file in turn. On partial failure, the project is in a half-renamed state. CLI prints which files succeeded and which didn't so the user can recover manually. Simplest, least safe.
2. **Stage and commit.** Write to temp files alongside the originals, then rename them into place. Final renames are nearly-atomic on most filesystems. Still not transactional but reduces the partial-failure window.
3. **Dry run + confirm.** `--dry-run` prints the rename plan without touching disk. Default could even be dry-run-with-confirm to make this hard to do by accident.

Lean: combine 2 and 3. Stage on disk, print a plan, then commit. `--dry-run` skips the commit. Open to other ideas.

## Other surfaces to update

- `views.yaml` doesn't reference item ids today, but a generic grep for the old id across config / template / view files would be defensive. At minimum: warn if the old id appears textually anywhere outside work item frontmatter.
- Templates: shouldn't normally reference ids — but check.

## Acceptance

- `workdown rename task-1 task-1-renamed` moves the file and rewrites every incoming link.
- Renaming to an existing id errors cleanly without changes.
- Invalid new id (uppercase, underscores, leading hyphen, etc.) errors cleanly.
- Items that had `parent: task-1` now have `parent: task-1-renamed`.
- Body of the renamed item unchanged.
- A summary lists each rewritten file.

## Open questions to think about during implementation

- Should `rename` also try to rewrite the *body* of items that mention the old id in prose (e.g. "see also task-1")? Body is freeform — risky to auto-rewrite. Probably no, but surface a warning when the textual form of the old id appears in any body.
- What about views/ rendered output (generated files in `views/`)? Those regenerate on next `workdown render` — probably fine to leave alone.
- Undo: just `workdown rename <new-id> <old-id>`. Worth saying out loud in the help text.
- Should this be its own milestone given the scope? Or stay here because it's still "an item mutation"? Lean: stay here, but if implementation balloons, lift out.
