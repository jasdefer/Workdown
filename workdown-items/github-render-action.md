---
id: github-render-action
type: issue
status: to_do
title: Reusable GitHub Action to keep rendered views in sync
parent: polish
depends_on: [next-release]
effort: "3h"
---

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
