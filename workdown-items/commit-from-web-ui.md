---
id: commit-from-web-ui
status: to_do
title: Commit changed items from the web app, with a proposed message
---

## In plain words

The web app can now pull and push, but it cannot commit — so the loop
is still broken in the middle. You move a card, the file changes, the
pill says "3 local", you press Push and nothing happens, because there
is no commit to push. The next step is a terminal. This item adds the
missing piece: a commit action with a message box that is already
filled in with a sensible sentence you can overwrite.

**Example:** you drag two items to `in_progress` and change one
assignee. The box opens pre-filled with what actually changed —
`fix-login-bug: status backlog → in_progress` — you adjust the wording
if you want, press Commit, and Push now has something to send.

Follow-up to [[git-sync-controls]], deliberately left out of PR #53 to
keep it focused.

## Where the idea comes from

Christian raised it in the PR under "Deferred follow-up (for
discussion)" and asked for design feedback before building:

> A **Commit** action in the UI, with a generated, editable message
> derived from the frontmatter diff (`fix-login-bug: status backlog →
> in_progress`) shown in a confirm dialog.

The point worth keeping is that the message is **proposed, not
demanded**. An empty text box makes every commit a small chore and
produces "update" as a message; a generated one derived from the diff
is usually right as-is and still fully editable.

## The decision this needs first

`CLAUDE.md` states: *"No auto-commit. CLI and UI mutations update the
working tree only. Staging and committing stay a user action — never
implicit."*

A button a person presses is an explicit action, so this does not break
the rule as written. But together with the pull and push already
shipped, it puts the whole git cycle in the web app while the CLI has
none of it — the same precedent question [[schema-editor-web]] raises
about `views.yaml`. Worth answering on purpose rather than by accident
a third time.

## Open questions

- **Scope of a commit.** All changed work items at once, or a
  selection. Selection means staging in the UI, which is a much bigger
  surface; all-at-once is honest and small, but only if the app is
  clear that it commits everything under the items directory.
- **Files outside the items directory.** The repository may be a whole
  code repo (the reason `git_controls` is opt-in at all). A commit from
  the board must never sweep up unrelated source changes — scoping the
  commit to the work-items path is probably the only safe default.
- **What the generated message says for more than one item.** One line
  per item does not fit a subject line; a summary subject plus a body
  listing the items probably does.
- **Commit and push in one gesture, or two.** Two is more predictable
  and matches what shipped; one is what people will ask for.
- **Where the diff comes from.** Frontmatter changes are the readable
  part, but a body edit is also a change and produces no field diff.
  The message needs to say something honest about those too.

## Notes

- The same opt-in flag and the same-origin guard from
  [[git-sync-controls]] apply — this is another mutating,
  network-adjacent endpoint.
- A commit is not undoable from the UI, so the confirm step is part of
  the feature, not polish.
