---
id: shared-git-crate
title: One git layer for the server and the CLI, in its own crate
status: done
parent: full-git-loop
tags:
- code-quality
---

## In plain words

Workdown talked to git in three places that did not share code. The
server had the real git layer — running git, deciding which paths the
git controls may touch, building the commit preview — written in the
web server's async style. `workdown changes` needed the preview, so the
CLI imported it from the server crate and started an async runtime just
to call it. `workdown install-hooks` ran its own `git rev-parse` with
plain code, separately from everything else.

Two things were wrong. The git layer sat in the wrong crate: it has
nothing to do with HTTP, and a terminal command reaching through the
web-server crate to run git reads as a layering mistake. And the layer
was async only because its first caller was, which forced every later
caller — the CLI now, the `prepare-commit-msg` hook next — to carry a
runtime it does not want.

Found during the review of PR #57 ([[commit-from-web-ui]]).

## Decisions taken (2026-09-08)

**1. Where the git layer lives — its own crate, `workdown-git`.**
Options weighed:

- *Leave it in the server crate.* Zero work; the CLI already depends
  on the server for `serve`. But the layering stays odd, `install-hooks`
  keeps its private git call, and every future git-touching CLI command
  deepens the dependency on the web server.
- *Move it into core.* Consumers are already there. But core is the
  pure "model, parse, compute" crate with no process spawning and no
  external binaries, and it is easy to test because of that. A
  component that shells out to `git` blurs a boundary that has served
  well.
- *A new crate between core and the front ends.* **Chosen.** It depends
  on core (config, path roles, the message generator); the CLI and the
  server both depend on it and not on each other for git. Core stays
  pure, the server stays HTTP-only, `install-hooks` drops its private
  `rev-parse`. Cost: one more workspace member and a docs update.

**2. Synchronous, not async.** Every git library and git-shelling tool
we know of (git2, gitoxide, cargo's git handling, the GitHub CLI)
exposes a blocking interface: waiting on an external process is
blocking work however it is dressed. Async servers run such work on a
side thread, which is what the server does now — one
`spawn_blocking` per handler, with the whole action (checks, git calls,
report) running as plain code on that thread. The CLI calls the same
functions directly, no runtime. The per-command timeout that the async
runtime used to provide is a wait-with-deadline on the child process,
with both output pipes drained on their own threads so a chatty child
cannot deadlock.

**3. What moved.** `crates/server/src/git.rs` → the crate root
(`workdown_git::snapshot`, `::commit`, …); `git_scope.rs` →
`workdown_git::scope`; `git_preview.rs` → `workdown_git::preview`. The
hook installer's `rev-parse` became `workdown_git::hooks_directory` and
`repository_prefix`. The scope cache in the server state is a plain
`OnceLock` now that nothing there is async. Behaviour is unchanged; the
existing unit and integration tests are the proof.

## Not in scope

- A git *library* (git2 / gitoxide) instead of the user's `git`. The
  reasons for shelling out stand: credentials, hooks and signing behave
  exactly as in the user's terminal. See [[git-sync-controls]].
- Making the mutating git surface available as a `workdown`
  subcommand. Decided against in [[full-git-loop]].
