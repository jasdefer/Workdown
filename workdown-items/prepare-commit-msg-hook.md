---
id: prepare-commit-msg-hook
title: Prefill terminal commits with the generated message
status: to_do
parent: misc-work
depends_on:
  - commit-from-web-ui
---

## In plain words

The web app's "Commit & push" button writes commit messages like
`Implement login: Status → In Progress` or `Move 2 items to Done`. A
commit made in a terminal after the same board work starts from an
empty editor, and history ends up half worded, half `wip`. A git
`prepare-commit-msg` hook can drop the generated message into the
editor before the user sees it, so a terminal commit reads the same
as one from the board — and stays editable, as in the dialog.

Follow-up to [[commit-from-web-ui]], where it was listed under
"considered, not part of this item" once the message generator
existed in the core crate. It does now. Deliberately not a child of
[[full-git-loop]]: that milestone closed on 2026-09-08 with the loop
working from the board, and this item is a nicety for the terminal side
that should not hold it open.

## What is needed

Two pieces, because a git hook is a shell script and cannot call a
Rust function:

1. **A `workdown changes` command.** Prints the generated message for
   the working tree against `HEAD`, for the workdown paths only —
   the same scope rule and the same summary function the button uses.
   Useful on its own: "what would the button say right now" from a
   terminal, and a way to see the message before `git commit`.
   *Built 2026-09-07 in working-tree mode. The preview builder lives
   in the shared git crate (`workdown_git::preview`, see
   [[shared-git-crate]]) and the CLI calls it directly — no runtime,
   no dependency on the server. Stdout carries the message alone; the
   note about files outside the workdown paths goes to stderr, so a
   hook can pipe stdout into git's message file. `--files` lists the
   covered files first. The `--staged` mode below is still open.*
2. **The hook.** `workdown install-hooks` already writes a pre-commit
   hook behind a marker comment ([[init-install-hooks]]). This item
   adds a `prepare-commit-msg` hook by the same mechanism: our marker,
   idempotent reinstall, a foreign hook stops the install and prints
   the line to add by hand, a missing binary fails loudly. The hook
   runs `workdown changes` and writes the result into the message file
   git hands it.

## Open questions for the design round

- **When the hook stays quiet.** Git calls `prepare-commit-msg` for
  every commit, including merges, `-m` commits and amends. The hook
  should only prefill an otherwise empty message for a plain commit
  (the hook's second argument is empty then), and never overwrite a
  message the user gave with `-m` or a template.
- **Scope of the commit versus scope of the message.** A terminal
  commit may stage more than workdown files. The message should
  describe what is *staged* under the workdown paths, not what is
  dirty — so `workdown changes` needs a `--staged` mode (index against
  `HEAD`) alongside the default working-tree mode the button's preview
  corresponds to. Files outside the paths are not described; the user
  writes those lines.
- **Where the hook lives in `install-hooks`.** One command installing
  two hooks with one flag, or a `--message` flag to opt in separately.
  Leaning to install both by default: a repo that wants rendered views
  kept fresh almost certainly wants readable history too.
- **Whether `workdown changes` is also the dogfood tool** for the
  message generator: a quick way to check wording on a real repository
  without opening the browser.

## Not in scope

- Changing the message shape. The grouping rule and wording are
  decided in [[commit-from-web-ui]]; this item only carries them to a
  second place.
- A `commit-msg` hook that validates or rewrites what the user typed.
