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
they inherit the rule above automatically. Precise wording of the rule:
the button itself never stages anything outside the workdown paths.
Hooks the user installed may add to the commit as they always do — a
repo with the `install-hooks` pre-commit hook gets its re-rendered
views in the button's commit, which is what that hook is for.

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

### Message shape (revised 2026-09-04, second round)

A commit message is read later, as one line in a list, by someone
scanning history. A body that repeats the diff field by field is
noise: anyone who wants that detail opens the diff. So the generated
part is a good **subject line**, and a body appears only when the
subject cannot carry the content. The *why* cannot be generated and is
left to the person, who can type it into the editable message.

What the generator adds that the diff does not show: names the way a
person would say them. The diff shows `implement-login.md` and
`in_progress`; the message uses the item's title and the choice's
label from the schema.

```
Implement login: Status → In progress
Move 2 items to In progress
Assign 3 items to Alice
Update 4 work items
Edit schema.yaml
```

Grouping rule:

- One item, one field: `<Title>: <Field label> → <new value label>`.
- Several items, same field, same new value: collapse into one line
  with the count.
- Anything mixed: a count (`Update 4 work items`, `Update 3 work
  items, 1 added`). Only then does a body follow, one line per item
  (`<Title>: <field> → <new>`), capped at about ten lines with the rest
  summarized by count.
- A changed body with no frontmatter difference: "description edited".
- Definition files (`schema.yaml`, `views.yaml`, `resources.yaml`,
  `config.yaml`, templates): one line each, "edited"/"added"/"deleted".
  No field-level comparison; it buys little.

The generator knows field names and choice labels from the schema and
nothing else. It may say "Status → In progress"; it may never say
"Started", because no field is privileged except `id`.

### Where the summary comes from

Not from parsing diff text. Git's unified diff is line-oriented, so a
multi-line list value or a reordered field would produce misleading
pairs. Git supplies which files changed and the old text of each one
(the `HEAD` blob); the working tree supplies the new text. One function
in the core crate takes old text, new text and the schema, parses both
frontmatters with the parser the tool already has, and compares field
by field. Added and deleted items fall out of the same comparison with
one side empty. The server calls that function in-process; no CLI
round trip, no journal, no bookkeeping anywhere.

### Mechanics that follow from the decisions

- **Path-scoped add and commit.** A plain `git commit` commits the
  whole index, so anything the user staged in a terminal would be swept
  up. Both the add and the commit are limited to the in-scope paths.
  Everything else stays staged or dirty exactly as it was.
- **Behind the remote.** The sequence is commit, then pull with rebase,
  then push. **The pull step is skipped when the branch is not
  behind**; only a branch that actually has remote commits to
  integrate pays for it. Pull still refuses over uncommitted changes,
  and out-of-scope dirty files count — so on a code repo with
  unrelated edits *and* a branch that is behind, the button commits,
  cannot pull, and stops with a clear message naming the files outside
  workdown. Acceptable: the audience is shared item repos, where the
  tree is clean after the commit by construction.
- **Conflicts on rebase.** Git merges by line and treats changes on
  touching lines as one contested block, so two people editing
  *different* fields of the *same* item within one sync window conflict
  whenever those fields sit on neighbouring lines (verified 2026-09-04
  in a scratch repo: `status` and `assignee` on adjacent lines
  conflict; with a line between them they merge). Different items
  never conflict. Decided: this is rare enough to accept, and a
  terminal or git GUI resolves it in a minute. The button aborts the
  rebase (helper from [[git-sync-controls]]), keeps the commit local,
  and the message says three things plainly: the commit is safe and
  local; it could not be combined with a teammate's change to item X;
  resolve in a terminal and press Push. Nothing is lost.
- **Missing identity.** If `user.name` or `user.email` is unset, refuse
  with a clear message. Never invent one.
- **Hooks and signing.** Already non-interactive from
  [[git-sync-controls]]; a signing prompt fails fast rather than
  hanging. That failure needs a readable message, not raw stderr.
- **The pill number is the number of things the button will commit.**
  "11 local" that mixed config, rendered views and item edits is what
  made the first reading confusing. The dirty count is computed over
  the workdown paths only; files outside them do not count at all and
  are not shown on the pill. They are named only where they matter:
  in the confirmation dialog and in the "cannot pull" message. Count
  items rather than files, and name definition files separately
  ("3 items · schema" rather than "4 local").
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

## Considered, not part of this item

- **Commit per board gesture** instead of a batch, with Push as the
  review moment listing the messages about to leave. Messages would be
  trivially meaningful because each commit *is* one gesture. Loses on
  three points: edits made in an editor or via the CLI would never be
  committed and would block pull; a drag and drag-back produces two
  commits; a commit nobody asked for stretches "explicit". Batch stays.
- **Field-wise merge for work items.** The app knows the file format
  and git does not; merging frontmatter by field would dissolve the
  adjacent-line conflicts above and leave only true same-field
  disagreements, which could become a "keep which?" dialog. Good UX,
  separate deliverable, only worth filing if the conflicts turn out
  not to be rare.
- **`workdown changes` CLI command** exposing the same summary function
  for the working tree against `HEAD`, and a `prepare-commit-msg` hook
  prefilling terminal commits with it, so history reads the same
  whether the commit came from the board or the shell. The hook slot
  exists from [[init-install-hooks]]. Cheap follow-up once the function
  exists in core.

## Notes

- A commit is not undoable from the UI, so the confirmation dialog is
  part of the feature, not polish.
- **Verified 2026-09-04:** a path-scoped `git commit -- <items>` with a
  pre-commit hook that re-renders and stages a views file does include
  the hook's file in the commit, and leaves an unrelated dirty source
  file untouched. The hook rule above needs no special handling. (A
  leftover status entry in the scratch test was a CRLF artifact of the
  Windows setup; content was identical.)
- **Verified 2026-09-04:** schema choices carry no labels. `status` is
  a plain value list (`in_progress`), so the message prettifies raw
  values the way the title fallback prettifies filenames. No new
  schema concept.

## Implementation decisions (2026-09-04)

Recorded from the decision sheet at the end of the design session. The
maintainer left before confirming 2, 3 and 6 individually; the
recommended option is recorded for each and may be revisited before
that step is built. Non-Rust wording on purpose.

1. **Two requests: preview, then commit.** The dialog opens with a
   read-only "what would happen" request returning the in-scope file
   list and the generated message. Confirming sends a second request
   carrying the final message. The preview is harmless and repeatable.
2. **Stale preview is refused.** The confirmation carries the file list
   the user saw. If the in-scope set differs when the commit request
   arrives (another tab, an editor), the server refuses with a
   conflict status and the dialog reloads the preview. The dialog is
   the review moment; committing files the user did not see defeats
   it.
3. **Commit, pull, push are one server action.** Not three browser
   calls. The server holds the git lock across all steps, owns the
   skip-pull-when-not-behind rule, and returns a per-step report the
   dialog renders as a checklist: committed (hash), pulled N / skipped,
   pushed / stopped with reason. The existing pull and push endpoints
   stay for their own buttons.
4. **Scope is computed once and passed to every git call.** The path
   list comes from the config (`paths.*`, `schema`, `config.yaml`
   itself), expressed relative to the *repository* root, because a
   workdown project may live in a subfolder of a larger repository.
   Status, add and commit all receive the same list. No filtering
   after the fact.
5. **Naming reuses existing behaviour.** Item name: the field the
   config's title display role points to, falling back to the
   prettified filename, as the board does. Field names and choice
   values: prettified from raw text with the existing slug prettifier.
   No new config.
6. **Button rule in the pill.** In-scope changes present: "Commit &
   push" is the primary button; Pull is disabled with the existing
   "commit first" hint. Clean and ahead: Push. Clean and behind: Pull.
   Clean and in sync: branch only.
7. **Failures are worded, raw output on demand.** Known cases mapped to
   sentences: no `user.name`/`user.email`, signing prompt, hook
   failure, rebase conflict (three-part wording above), push rejected.
   Git's raw output sits behind a "details" toggle.
8. **The message generator is a pure function in the core crate.**
   Input: per file, old text (from `HEAD`) or none, new text (working
   tree) or none, plus schema and the title role. Output: the message.
   Testable without git or a server; reusable by a later `workdown
   changes` command. Server tests use the existing fixture repositories
   for scope filtering, stale-preview refusal, stop-at-pull with
   outside files dirty, and missing identity.

No new config key is introduced, so there is no release-ordering
problem this time.

### Build order

1. Core: the summary function with unit tests (grouping rule, body
   edits, added/deleted, definition files, cap).
2. Server: scope list from config; status endpoint counts in-scope only
   and reports items vs definition files; pill shows the new number.
3. Server: preview endpoint (files + message).
4. Server: commit-pull-push endpoint with per-step report, stale-set
   refusal, worded failures; same-origin guard and git lock as the
   other git endpoints.
5. UI: dialog (file list, editable message, checklist result) and the
   button rule.
6. Docs: short note in ADR-006; [[full-git-loop]] records "the CLI
   stays commit-free" as its own decision; `docs/architecture.md` if
   the mutations exit gains a stage.
7. Dogfood on this repo before release: the pill must show the
   in-scope count, and the button must stop cleanly at pull when
   `views/` is dirty and the branch is behind.
- Status changes are also where [[status-transition-dates]] wants to
  write a date. If both land, one board gesture produces a field write
  *and* a commit — worth designing so the date is part of the same
  saved change rather than a second one arriving behind it.
