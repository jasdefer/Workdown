---
id: commit-from-web-ui
title: Let the web app commit, so the git loop is not broken in the middle
status: in_progress
parent: full-git-loop
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

## Settled in discussion (2026-08-31)

A follow-up conversation between the two of us settled the outermost
question and sketched a shape for the rest. Recorded here so the design
work starts from it rather than re-deriving it.

**Decided — the final push is always the user's gesture.** Saving,
committing and publishing are not one button. The reason is review: you
want a moment to look at what accumulated before it leaves your machine.
Both of us landed on this independently, and it matches what
[[git-sync-controls]] already shipped, where Push is its own control.
*Revised 2026-09-04: one button, with the review moment moved into a
confirmation dialog. See below.*

**Decided — a long, noisy history is an acceptable price.** These
commits are mechanical and numerous by nature ("status: to_do →
in_progress"), and neither of us thinks a tidy log is worth constraining
the feature for.

**Proposed, not decided — a Save button over batched changes.** Rather
than one commit per drag, you change several things and then save once.
Two variants came up: staging each mutation as it happens and committing
on save, or committing early and amending as more changes arrive. The
second is cheap to describe and unpleasant if anything else is touching
the repository, so it needs care rather than enthusiasm.
*Resolved 2026-09-04: neither variant. Nothing is staged or committed
until the button is pressed; the working tree is the batch.*

**Proposed, not decided — the message is generated from what changed.**
The everyday case really is one field moving on one item, which is
exactly the case a generated message handles well and a blank box
handles badly.
*Decided 2026-09-04, see below.*

## What was not decided (all resolved 2026-09-04, see below)

- ~~**Where the message comes from.**~~ Generated from the diff,
  editable in the confirmation dialog.
- ~~**How much a commit covers.**~~ Everything dirty inside the
  workdown paths. No selection UI.
- ~~**Files outside the items directory.**~~ Never touched. The scope
  is the set of paths `config.yaml` names, plus `config.yaml` itself.
- ~~Commit and push as one gesture or two.~~ One gesture behind a
  confirmation dialog.
- ~~**Body edits.**~~ Reported as "description edited" in the message.

## Observed on this repo (2026-09-02)

Git controls were switched on in this repo's own config
([[dogfood-git-controls-config]]). First reading of the pill's data,
taken from `GET /api/git` on the maintainer's machine:

- Branch `enhance-git`, **no upstream**, ahead 0, behind 0, **11 local**.
- The 11 dirty files were three unrelated pieces of work from three
  sessions: re-rendered `views/*.md`, two item edits from an earlier
  session, and the config change plus item rewrite that switched the
  feature on. The pill shows one number for all of them.
- With no upstream, Push has nothing to push to. The endpoint reports
  `has_upstream: false`; whether a board commit followed by Push should
  set the upstream, or refuse until a terminal does it, is a question
  the design has to answer for every fresh branch. *Answered since: Push
  publishes an unpublished branch itself (shipped on `enhance-git`,
  unreleased).*

What this says for the design: "commit everything dirty" is wrong on
the very first real reading. `views/` is generated output, the config
change is not item work, and the two item edits belong to a different
change than the config. Whatever the commit covers, the pill has to
show *which* files, not just how many.

## Settled in discussion (2026-09-04)

The shape is now decided. What follows is the design the implementation
starts from.

### Decisions

**One button: stage, commit, push.** A single control in the git pill
does all three. It opens a confirmation dialog first, which lists the
files about to be committed and shows the generated message, editable.
The dialog is the review moment the 2026-08-31 decision asked for; it
replaces the separate Push gesture for this flow. The existing Pull and
Push buttons stay for the cases where only one of them applies (tree
clean, commits ahead).

**Scope is the workdown paths, nothing else.** The commit covers every
dirty file — modified, added, deleted — under the paths `config.yaml`
names (`paths.work_items`, `paths.templates`, `paths.resources`,
`paths.views`, `schema`) plus `config.yaml` itself. No other file in the
repository is ever staged by the web app. This is the answer to the
"whole code repo" concern that made `git_controls` opt-in: source
changes sitting next to the items are invisible to the button.

**Rendered view output is out of scope.** The `views/*.md` files that
`workdown render` writes are generated, and no config path names them.
They are not committed by the button. If they later get a config path,
they inherit the rule above automatically.

**No selection.** Everything in scope goes in one commit. Building
staging into the UI is not worth it for mechanical changes; the scope
rule does the filtering that matters.

**The message is derived from the diff, not from a journal.** The
alternative — `workdown add`/`edit` and the server writing a small
change log for the commit to read — was considered and rejected. It has
two writers to keep in sync, it misses hand edits and terminal edits,
it has to be gitignored, it goes stale when someone commits from a
terminal, and two browser tabs would race on it. Everything it would
record is already recoverable from the repository: the core crate
parses frontmatter, so the server reads each changed file at `HEAD` and
in the working tree and diffs them field by field.

**Body edits are named honestly.** A changed Markdown body with no
frontmatter difference is reported as "description edited".

**The CLI stays commit-free.** This item answers the precedent question
[[full-git-loop]] owns: the web app may commit, scoped to the workdown
paths, only on an explicit confirmed gesture. The "no implicit commit"
rule in `CLAUDE.md` and ADR-006 stays true as written; ADR-006 gets a
short note recording this.

### Message shape

```
Update 3 work items, 1 added

implement-login: status to_do → in_progress
fix-bug: assignee → alice, description edited
new-onboarding-task: added
schema.yaml edited
```

Summary line first: counts by kind (changed, added, deleted items;
definition files touched). Then one line per item with its field
changes, in the order `id: field old → new`. Definition files
(`schema.yaml`, `views.yaml`, `resources.yaml`, `config.yaml`,
templates) get a one-line "edited"/"added"/"deleted". The detail list is
capped at about ten lines; the rest is summarized by count. The whole
thing is editable before it is committed.

### Mechanics that follow from the decisions

- **Path-scoped add and commit.** A plain `git commit` commits the
  whole index, so anything the user staged in a terminal would be swept
  up. Both the add and the commit are limited to the in-scope paths.
  Everything else stays staged or dirty exactly as it was.
- **Behind the remote.** The sequence is commit, then pull with rebase,
  then push. If the rebase conflicts, abort it (the helper exists from
  [[git-sync-controls]]), keep the commit local, and say so. Pull still
  refuses over uncommitted changes, and out-of-scope dirty files count
  — so on a code repo with unrelated edits the button commits, cannot
  pull, and stops with a clear message. Acceptable: the audience is
  shared item repos, where the tree is clean after the commit by
  construction.
- **Missing identity.** If `user.name` or `user.email` is unset, refuse
  with a clear message. Never invent one.
- **Hooks and signing.** Already non-interactive from
  [[git-sync-controls]]; a signing prompt fails fast rather than
  hanging. That failure needs a readable message, not raw stderr.
- **The pill shows in-scope changes.** "11 local" that mixed config,
  rendered views and item edits is what made the first reading
  confusing. The status reports the in-scope count (and which files),
  separately from anything else dirty in the repository.
- **Opt-in and origin guard.** Same as pull and push:
  `serve.git_controls` must be on, and the endpoint is same-origin
  guarded. See [[same-origin-guard-everywhere]].

## The question underneath

`CLAUDE.md` states: *"No auto-commit. CLI and UI mutations update the
working tree only. Staging and committing stay a user action — never
implicit."*

A button a person presses is explicit, so this does not break the rule
as written. But with pull and push already shipped, it puts the whole
git cycle in the web app while the CLI has none of it — the same
precedent question [[schema-editor-web]] raises about `views.yaml`.

That question is bigger than this feature and belongs to the milestone,
[[full-git-loop]], which exists to answer it once. The answer this item
proposes is recorded above under "The CLI stays commit-free"; the
milestone should carry it as its own decision.

## Notes

- A commit is not undoable from the UI, so the confirmation dialog is
  part of the feature, not polish.
- Status changes are also where [[status-transition-dates]] wants to
  write a date. If both land, one board gesture produces a field write
  *and* a commit — worth designing so the date is part of the same
  saved change rather than a second one arriving behind it.
