# ADR-014: The git surface — what workdown may touch, on whose gesture, from where

**Status:** Accepted
**Date:** 2026-09-08

## Context

ADR-006 ruled that mutations update the working tree only and that
staging and committing are explicit user actions. Pull and push buttons
then shipped in the web UI, and the loop was broken in the middle: the
board could fetch and publish but never commit, so every board session
ended in a terminal. Closing that loop meant deciding, once, how much
of a user's repository workdown may touch and who may do it — the
repository is often the user's whole code repo, not an item store. The
decision was worked out in `workdown-items/full-git-loop.md`,
`commit-from-web-ui.md` and `shared-git-crate.md`; this ADR records the
result so it outlives those items.

## Decisions

### The web app may commit; the CLI may not

`workdown add`, `set`, `unset`, `move`, `rename` and `body` touch the
working tree only, as they always have. The web app's git pill may
pull, push, and — behind a confirmation dialog — stage, commit and
push. ADR-006's "no auto-commit" rule stays true as written: nothing is
staged or committed until the user has seen the file list and the
message and pressed the button. No board gesture commits on its own.

This is the one exception to ADR-006's "UI is a shell around the CLI":
the mutating half of the git surface (commit, pull, push) has no
`workdown` subcommand twin — the terminal is that twin. The read-only
half has one: `workdown changes` prints the message the dialog would
propose.

### Only the workdown paths

A commit from the browser covers the paths `config.yaml` names
(`paths.*`, `schema`) plus `config.yaml` itself — `Config::workdown_paths`
is the one definition, shared with the pre-commit hook installer. No
other file in the repository is ever staged by the app, whatever the
user has dirty next to the items. The scope is computed once per
process as repository-relative pathspecs (`workdown_git::scope`), so
membership is git's own pathspec answer everywhere and never an
after-the-fact filter. Files outside the scope are named only where
they matter — a pull that must refuse over uncommitted work — and
otherwise do not appear.

### The user's own `git`, in its own crate

Workdown runs the `git` on `PATH`, not a git library, so credentials,
hooks and signing behave exactly as in the user's terminal; a hook may
add to a commit as it always does. Every invocation is non-interactive
and bounded by a timeout.

That layer is `crates/git` (`workdown-git`): synchronous, depending on
core, depended on by both the CLI and the server. Core stays the pure
model-parse-compute crate with no process spawning; the server stays
HTTP-only and runs each git action on a blocking thread under one lock.
Waiting on an external process is blocking work however it is dressed,
so the crate is not async — the server pays for that at its edge, and
the CLI calls the same functions directly.

### The message comes from parsed frontmatter, not diff text

The generated commit message (`workdown_core::change_summary`) compares
each changed item's fields at `HEAD` and in the working tree, worded
from the schema: item titles instead of filenames, prettified field
names and values instead of slugs. It knows field names, types and
values and nothing else — it may say "Status → In Progress", never
"Started", because no field is privileged except `id`. A unified diff
is line-oriented and would mislead on a reordered field or a multi-line
value; parsing both sides with the parser the tool already has cannot
be fooled that way.

## Consequences

- `serve.git_controls` is safe to switch on in a code repository: a
  source change sitting next to the items is invisible to the button.
- Anything git-shaped a future CLI command needs comes from
  `workdown-git`, never from the server crate.
- A terminal commit and a board commit can read the same in history;
  `prepare-commit-msg-hook` carries the message to the terminal side.
