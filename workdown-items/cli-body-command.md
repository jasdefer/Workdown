---
id: cli-body-command
type: issue
status: to_do
title: workdown body — edit the Markdown body
parent: item-mutations
---

Frontmatter mutations cover structured fields. The Markdown body below the frontmatter is freeform and needs its own command — the UI's card detail view will want a description editor, and scripts will want to append notes.

```
workdown body <id> --set "<markdown>"
workdown body <id> --append "<markdown>"
workdown body <id> --edit
```

## Initial idea

- `--set` replaces the entire body with the given Markdown. Reads from stdin when value is `-` (the standard "read from pipe" convention) so multi-line content is ergonomic.
- `--append` appends Markdown to the existing body (with a separating blank line).
- `--edit` opens `$EDITOR` on a temp file pre-populated with the current body, writes back on close. Falls back to a sensible default editor on Windows / macOS / Linux if `$EDITOR` is unset (or errors with a clear message — open question).
- No validation: the body is freeform Markdown by design. The file's frontmatter is left untouched.
- Errors: unknown item id, I/O failure, editor failure (e.g. user exits with non-zero status). The `--edit` path needs special handling — if the user empties the file or cancels, what should we do? Open question.

## Core function

```rust
pub fn run_body_edit(
    config: &Config,
    project_root: &Path,
    id: &WorkItemId,
    operation: BodyOperation,  // Set(String) | Append(String) | Replace(String)
) -> Result<BodyOutcome, BodyError>;
```

The `--edit` mode is CLI-side only — it launches the editor, captures the result, and then calls `run_body_edit` with `BodyOperation::Set(new_content)`. Keeps the core function pure and reusable by the server.

`BodyOutcome` probably needs `previous_body`, `new_body`, `path` — though `previous_body` could be large; consider returning a byte count or a diff summary instead. Open question.

## Acceptance

- `workdown body task-1 --append "## Notes\nLooks good."` adds the text to the end of the body.
- `workdown body task-1 --set "Fresh description"` replaces the body.
- `workdown body task-1 --set -` reads from stdin.
- `workdown body task-1 --edit` opens an editor; saving and closing persists the changes.
- Frontmatter is byte-identical before and after.

## Open questions to think about during implementation

- Should `--append` separator be configurable, or fixed as a blank line? Some users may want a Markdown horizontal rule.
- `--edit` cancel semantics: if the user exits without saving, do we no-op or error? Lean: no-op if the file is unchanged, save if it changed.
- Length limits / sanity checks (gigabyte paste)? Probably skip — trust the user.
- Should the CLI print a body-diff on completion or just "Updated task-1 body (+3 lines, -1 line)"? Lean: a line count summary, full diff only on `--verbose`.
- Future: a `workdown body <id> --show` for stdout-friendly viewing without involving cat / less. Or is that out of scope here?
