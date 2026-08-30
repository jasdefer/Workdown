---
id: commit-from-web-ui
status: to_do
title: Let the web app commit, so the git loop is not broken in the middle
---

## In plain words

The web app can pull and push, but it cannot commit — so the loop stops
halfway. You move a card, the file changes, the pill says "3 local",
you press Push and nothing happens, because there is no commit to
push. The next step is a terminal.

This item says only that the gap should close. **How** it closes is not
decided here.

Follow-up to [[git-sync-controls]], deliberately left out of PR #53 to
keep that one focused.

## Why it is worth doing

Everything else about working from the board is self-contained: you
drag, it saves, other tabs update. The moment you want that work to
leave your machine, you drop out of the app for one command and come
back. For the audience this was built for — teams sharing an item repo,
where the whole point was that people forget the git step — that is the
exact step they forget.

## What is not decided

- **Where the message comes from.** A plain text box is the obvious
  first version and is cheap. Christian's proposal in the PR was a box
  pre-filled from what actually changed (`fix-login-bug: status backlog
  → in_progress`), still fully editable, on the grounds that these
  commits are usually mechanical and a blank box tends to produce
  "update". Both are defensible; nothing here picks one.
- **How much a commit covers.** Everything changed at once, or a
  selection. Selection means building staging into the UI.
- **Files outside the items directory.** The repository may be a whole
  code repo — that is why `git_controls` is opt-in at all. A commit
  from the board must never sweep up unrelated source changes. This is
  the one where a wrong answer does damage.
- **Commit and push as one gesture or two.**
- **Body edits.** A changed Markdown body is a real change that
  produces no frontmatter difference. Whatever the message does, it has
  to be honest about those.

## The question underneath

`CLAUDE.md` states: *"No auto-commit. CLI and UI mutations update the
working tree only. Staging and committing stay a user action — never
implicit."*

A button a person presses is explicit, so this does not break the rule
as written. But with pull and push already shipped, it puts the whole
git cycle in the web app while the CLI has none of it — the same
precedent question [[schema-editor-web]] raises about `views.yaml`.
Worth answering deliberately rather than by accident a third time.

## Notes

- The opt-in flag and the origin check from [[git-sync-controls]] apply
  — this is another mutating endpoint. See
  [[same-origin-guard-everywhere]].
- A commit is not undoable from the UI, so a confirmation step is part
  of the feature, not polish.
