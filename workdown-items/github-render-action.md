---
id: github-render-action
status: removed
title: Reusable GitHub Action to keep rendered views in sync
parent: polish
---

> **Removed 2026-08-03, no consumers.** Kept as a record of the idea,
> not as work to do.
>
> A reusable action exists for other teams; there are none yet. Building
> it now would ship infrastructure with zero users, no real-world test
> case, and a maintenance surface — the same merits reasoning that
> dropped the release notes. The one real staleness problem (this
> repo's own `views/`) is solved at the source by installing the
> [[init-install-hooks]] hook locally; a stale-views check in this
> repo's own `ci.yml` is the cheap next step if stale views still slip
> through. Extract the reusable action when a real team adopts workdown
> and there is an actual consumer to test against.

Teams that skip the local hook ([[init-install-hooks]]) need the CI
variant: a reusable GitHub Action consumer repos reference from their
own workflows, so committed rendered views never drift from the items.
Binary acquisition is solved — cargo-dist publishes installers and
per-platform binaries on every GitHub release, so the action downloads
instead of compiling.

## Open decisions

1. Sync mode: the action commits the re-rendered views back to the
   branch, or fails the check and asks the author to render locally.
2. Where the action lives: `action.yml` in this repo (referenced as
   `jasdefer/Workdown@<tag>`) versus a separate repo.
3. Version pinning: the action installs the latest release or a
   consumer-pinned version.

## Out of scope

- A full CI template for consumer repos (validate, etc.) — the action
  does one job: render sync.
