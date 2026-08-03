---
id: init-install-hooks
type: issue
status: to_do
title: Optional --install-hooks for pre-commit render
parent: polish
effort: "3h"
---

Rendered views (`workdown render` output committed to the repo) go
stale the moment an item changes. A pre-commit hook that re-renders
closes the gap locally, without requiring CI. Opt-in: `workdown init
--install-hooks` writes the hook; plain `init` never touches
`.git/hooks`.

## Open decisions

1. What the hook does: re-render and stage the views automatically, or
   fail the commit when views are stale and let the user re-render.
2. What happens when a pre-commit hook already exists.
3. Whether the flag is `init`-only or also available to existing
   projects (e.g. a standalone `workdown install-hooks`).
4. What the hook script requires at runtime (a `workdown` on PATH; how
   it behaves when the binary is missing).

## Out of scope

- CI-side sync — that is [[github-render-action]].
