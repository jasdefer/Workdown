---
id: full-git-loop
status: to_do
title: The full git loop, without leaving the board
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

## Scope

- [[commit-from-web-ui]] — the missing step, and the design work that
  decides its shape.
- [[dogfood-git-controls-config]] — turn the shipped feature on in this
  repo, so the next piece is designed against something we actually
  use.
- [[git-sync-controls]] — done, parented here retroactively so the
  shipped half and the unshipped half sit together.

## Not in scope

- [[same-origin-guard-everywhere]] stays standalone. It came out of the
  git PR review, but it is about `POST /timer/stop` and
  `POST /timer/break/end`; git is merely where the fix already exists.
  Filing it here would hide a security item behind a feature.
- The fetch-and-read-blobs plumbing [[multi-project-support]] needs.
  That is git as an implementation detail of another deliverable, and
  splitting it out would fight the milestone that owns it.
- CI and GitHub Actions work. Different tool, different problem.
