---
id: publish-branch-from-push
title: Let Push publish an unpublished branch instead of greying out
status: to_do
parent: full-git-loop
---

## In plain words

On a branch that has never been pushed, the web app's Push button is
disabled with the tooltip "No upstream branch configured", Pull is
disabled for the same reason, and the pill reads **in sync** — which
is false: nothing has left the machine. The way out is a terminal and
`git push -u origin <branch>`, the exact step the git controls exist to
remove.

From the user's side, push and publish are one gesture: get my commits
onto the remote. The first time, git also has to create the remote
branch and remember it as the upstream. That is a detail the app can
handle, not a reason to send people to a terminal.

Found on 2026-09-02 while dogfooding ([[dogfood-git-controls-config]]):
the first branch the controls were tried on had no upstream, and the
pill reported "in sync" over two unpushed commits.

## Decisions

1. **Same button, label follows the situation.** "Push" when the branch
   has an upstream, "Publish" when it does not. Same slot, same styling,
   same flow — the pattern VS Code and GitHub Desktop use. The tooltip
   names what will happen: "Publish enhance-git to origin".
2. **"In sync" is never shown for an unpublished branch.** The summary
   reads **not published** instead. "In sync" tells a user their work is
   safe when it has never left the machine; that is the one outright
   wrong behaviour here.
3. **The server picks the remote by git's own rule.** `remote.pushDefault`
   if set; otherwise the only remote if there is exactly one; otherwise
   `origin` if it exists. When none applies the button stays disabled
   with "No remote configured" — several remotes and no default is rare
   enough that a tooltip pointing at a terminal is acceptable.
4. **Publish sets the upstream as it pushes** (`git push -u <remote>
   <branch>`). The pill's next refresh then has real ahead/behind numbers
   and the button reads "Push" from there on.
5. **No extra confirmation on first publish.** Creating a remote branch
   is more visible than pushing to an existing one, but a clearly
   labelled button the user clicks is explicit enough. Cheap to add
   later if it turns out to be wanted.
6. **Pull stays disabled without an upstream** — there is nothing to
   pull from. It enables itself once the branch is published.

## Refused cases

- A branch with no commits (unborn, fresh `git init`) cannot be
  published; the button says so.
- A detached head has no branch name to publish; the button says so.

## Scope

- Status endpoint: expose enough for the pill to tell "no upstream, a
  remote exists" from "no remote at all" — most likely the resolved
  remote name, or `null`.
- Push endpoint: when the branch has no upstream, publish with `-u` to
  the resolved remote instead of running a bare `git push`; refuse on
  unborn branch and detached head with a clear message.
- Pill: label, summary text and enabled rule per the decisions above;
  tooltip carries the remote name.
- Tests on both sides, including the three remote-resolution cases and
  the two refusals.

## Notes

- Same-origin guard and the git lock apply, as for the existing push.
- Independent of [[commit-from-web-ui]]; ships on its own. That item
  inherits the "not published" state for its own pill text.
