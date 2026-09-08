---
id: full-git-loop
title: The full git loop, without leaving the board
status: done
---

## In plain words

You move a card from To do to In progress. The file changes, the pill
says "1 local", and then the app stops being useful: to get that change
anywhere, you open a terminal. The board can already pull and push —
the one step in the middle, committing, is missing, so the loop is
broken exactly where the everyday work happens.

This milestone is for closing that loop, and for settling the question
underneath it: **how much of a user's git repository is workdown
allowed to touch, and on whose gesture?** That question is currently
answered three different ways in three different places, which is the
real reason these items belong together.

## Why this is a milestone and not a tag

"Git" on its own is a theme, and themes belong in `tags`. What makes
this a deliverable is that one policy is spread across items that each
own a piece of it and none of which owns the policy:

- `CLAUDE.md` and ADR-006 state the rule: *"All mutations (CLI and UI)
  update the working tree only. Staging and committing are always
  explicit user actions, never implicit."*
- [[git-sync-controls]] shipped pull **and push** from the web UI in
  0.2.5. A button a person presses is explicit, so this does not break
  the rule as written — but it does put most of the git cycle in the
  app.
- [[commit-from-web-ui]] would add the last step, and names the
  precedent question without answering it.
- [[multi-project-support]] states as a settled decision that "the CLI
  and web app never commit or push" — written before 0.2.5 and no
  longer true. Corrected there, recorded here.

Answering that once, deliberately, is this milestone's job. The
individual features are downstream of it.

## Decision (2026-09-07)

**How much of a user's repository may workdown touch, and on whose
gesture?** Answered as follows; ADR-006 carries a note pointing here.

- **The web app may commit, the CLI may not.** `workdown add`, `set`,
  `unset`, `move`, `rename` and `body` touch the working tree only, as
  they always have. The web app's git pill may pull, push, and commit.
- **Only on an explicit, confirmed gesture.** Nothing is staged or
  committed until the user has seen the file list and the message in
  the dialog and pressed the button. No board gesture ever commits on
  its own.
- **Only the workdown paths.** The commit covers the paths
  `config.yaml` names (`paths.*`, `schema`) plus `config.yaml` itself.
  No other file in the repository is ever staged by the app, whatever
  the user has dirty next to the items. Files outside those paths are
  named where they matter (a pull that has to refuse) and nowhere else.
- **The user's own `git`.** The app shells out to the `git` on the
  machine, so credentials, hooks and signing behave as in a terminal;
  a hook may add to a commit as it always does.

The "no auto-commit" rule in ADR-006 stays true as written. The "UI is
a shell around the CLI" principle gains its one exception: the
mutating half of the git surface (commit, pull, push) has no `workdown`
subcommand twin, the terminal is that twin. The read-only half has one:
`workdown changes` prints the message the dialog would propose.

## Scope

- [[commit-from-web-ui]] — the missing step, and the design work that
  decides its shape.
- [[dogfood-git-controls-config]] — turn the shipped feature on in this
  repo, so the next piece is designed against something we actually
  use.
- [[git-sync-controls]] — done, parented here retroactively so the
  shipped half and the unshipped half sit together.
- [[publish-branch-from-push]] — found while dogfooding: Push on a
  branch that has never been pushed publishes it instead of greying out.
- [[shared-git-crate]] — came out of the PR #57 review: one git layer
  in its own crate, shared by the server and the CLI.

Closed 2026-09-08 with the first commit made from the board, on this
repository, with the generated message.

## Not in scope

- [[prepare-commit-msg-hook]] — prefilling *terminal* commits with the
  generated message. A nicety for the other side of the loop; kept out
  from under this milestone so it can close.
- [[same-origin-guard-everywhere]] stays standalone. It came out of the
  git PR review, but it is about `POST /timer/stop` and
  `POST /timer/break/end`; git is merely where the fix already exists.
  Filing it here would hide a security item behind a feature.
- The fetch-and-read-blobs plumbing [[multi-project-support]] needs.
  That is git as an implementation detail of another deliverable, and
  splitting it out would fight the milestone that owns it.
- CI and GitHub Actions work. Different tool, different problem.
