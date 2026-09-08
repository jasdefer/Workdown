---
id: publish-branch-from-push
title: Let Push publish an unpublished branch instead of greying out
status: done
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
   names what will happen: "Publish enhance-git".
2. **"In sync" is never shown for an unpublished branch.** The summary
   reads **not published** instead. "In sync" tells a user their work is
   safe when it has never left the machine; that is the one outright
   wrong behaviour here. A detached head reads **detached** for the same
   reason.
3. **The server picks the remote, at click time.** `remote.pushDefault`
   if it names an existing remote; otherwise the only remote if there
   is exactly one; otherwise `origin` if it exists. This is one step
   friendlier than bare `git push`, which never falls back to a sole
   remote — the step VS Code takes. When none applies the click is
   refused with "No remote configured". Several remotes and no default
   is rare enough that a message pointing at a terminal is acceptable.
4. **The status carries no remote name.** One remote is the common
   case, and the pill's job is the gesture, not the routing. Nothing new
   on the wire for the status; the pill decides Push vs Publish from
   `has_upstream`, which it already has.
5. **Publish sets the upstream as it pushes** (`git push -u <remote>
   <branch>`). The pill's next refresh then has real ahead/behind numbers
   and the button reads "Push" from there on.
6. **The push response says which of the two happened** — a `published`
   flag beside the fresh status, so the toast can read "Published
   enhance-git" rather than "Pushed". Mirrors how pull already returns
   `pulled_commits` beside its status.
7. **No extra confirmation on first publish.** Creating a remote branch
   is more visible than pushing to an existing one, but a clearly
   labelled button the user clicks is explicit enough. Cheap to add
   later if it turns out to be wanted.
8. **Pull stays disabled without an upstream** — there is nothing to
   pull from. It enables itself once the branch is published.
9. **No ahead count on an unpublished branch.** Git has no honest number
   without an upstream; counting every commit on the branch would
   include the whole history it branched from. Publish is enabled
   without a count.

## Refused cases

Refusals happen on the server, when the button is clicked, with a
message the pill shows:

- A branch with no commits (unborn, fresh `git init`) cannot be
  published.
- A detached head has no branch name to publish. The pill also knows
  this one up front (the branch reads `HEAD`) and keeps the button
  disabled.
- No remote can be chosen by the rule in decision 3.

An upstream that is configured but gone (remote branch deleted and
pruned) already reads as "no upstream"; Publish recreates it, which is
what the click asks for.

## Scope

- Push endpoint: when the branch has no upstream, publish with `-u` to
  the resolved remote instead of running a bare `git push`; refuse on
  unborn branch, detached head and no resolvable remote with a clear
  message. Return `published` beside the status.
- Pill: label, summary text, enabled rule and tooltips per the
  decisions above; toast text per the response.
- Tests on both sides, including the three remote-resolution cases and
  the three refusals.

## Notes

- Same-origin guard and the git lock apply, as for the existing push.
- Keep it simple: this is a web-app git control for the everyday case.
  Anything past that (several remotes, no default) is the terminal's or
  a git tool's job.
- Independent of [[commit-from-web-ui]]; ships on its own. That item
  inherits the "not published" state for its own pill text.
