---
id: cli-body-command
type: issue
status: done
title: workdown body — replace the Markdown body
parent: item-mutations
---

Frontmatter mutations cover structured fields. The Markdown body below the frontmatter is freeform and needs its own command — the UI's card detail view will write the description through it, and scripts will occasionally want to overwrite the body programmatically. For interactive editing the user opens the `.md` file directly; the CLI command exists for the non-interactive cases.

## Surface

```
workdown body <id> <markdown>
```

- Single positional value, always a full replacement of the body.
- No `--edit` (open the file in your editor instead), no `--append` (read + concat + replace in the caller if needed), no stdin support.
- Empty value (`workdown body task-1 ""`) is valid and clears the body; the frontmatter stays.
- The CLI command is a thin wrapper over a pure core function so the future server can call the same code path.

## File hygiene

The body is always stored with exactly one trailing `\n`. The CLI normalises:

- `"hello"` → `hello\n`
- `"hello\n\n\n"` → `hello\n`
- `""` → no trailing newline (file ends after the closing frontmatter `---`)

Reason: hand-edited Markdown files end with `\n` by convention. Picking one rule keeps diffs quiet when users switch between CLI writes and editor saves.

## Core function

```rust
pub fn run_body_replace(
    config: &Config,
    project_root: &Path,
    id: &WorkItemId,
    new_body: String,
) -> Result<BodyOutcome, BodyError>;

pub struct BodyOutcome {
    pub path: PathBuf,
    pub previous_body: String,
    pub new_body: String,
    pub warnings: Vec<Diagnostic>,
}
```

Bodies are small Markdown blobs, so returning both strings is cheap and matches the shape of `SetOutcome` (`previous_value` / `new_value`). The server will want `new_body` to echo into the UI; the CLI computes its summary from the strings.

## CLI output

```
task-1: body replaced (12 lines)
```

Line count of the new body. Line deltas (`-3 / +5`) aren't meaningful for a full replacement — the user can read the file to see what's there.

## Behaviour

- Loads the whole store (same as other mutations) for post-write diagnostics.
- Frontmatter bytes must be byte-identical before and after.
- Schema violations after the write follow the save-with-warning rule (ADR-001): write succeeds, warnings are surfaced, exit code is non-zero.
- I/O or parse errors on the target file hard-fail without writing.
- Unknown id is an error.

## Acceptance

- `workdown body task-1 "Fresh description"` replaces the body and prints `task-1: body replaced (1 line)`.
- `workdown body task-1 ""` clears the body. The file ends with `---\n` (closing frontmatter delimiter, no extra trailing newline).
- `workdown body task-1 "line one\nline two\n\n\n"` is stored as `line one\nline two\n`.
- Frontmatter bytes are byte-identical before and after every call.
- All store-wide warnings (`Store::load` + `rules::evaluate`) are printed after the mutation, same as `set` / `unset`.
- Unknown id exits non-zero with a clear error and writes nothing.
