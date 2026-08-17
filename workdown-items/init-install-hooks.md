---
id: init-install-hooks
status: done
title: Optional --install-hooks for pre-commit render
parent: polish
---

Rendered views (`workdown render` output committed to the repo) go
stale the moment an item changes. A pre-commit hook that re-renders
closes the gap locally, without requiring CI. Opt-in: `workdown init
--install-hooks` writes the hook; plain `init` never touches
`.git/hooks`.

## Decisions taken (2026-08-03)

1. **Both modes ship.** The default hook re-renders and stages, so the
   fresh views land in the same commit (pre-commit runs before the
   commit object exists — the formatter pattern, no second commit). A
   `--check` variant fails the commit when views are stale and leaves
   staging to the user. Either way the hook exits early when the staged
   changes touch neither the work items nor `.workdown/` — an
   unconditional render would make every commit pay the render cost and
   would sneak `$today` date-drift into unrelated commits.
2. **An existing pre-commit hook stops the command**, which prints the
   line to add manually. Editing a foreign hook script automatically is
   a bigger risk than one paste. Exception: a hook carrying our own
   marker comment is overwritten, so reinstalls and mode switches stay
   idempotent.
3. **Dedicated `workdown install-hooks` command; `workdown init
   --install-hooks` delegates to it** with the default mode. `init` is
   already a safe no-op on initialized projects, so the flag serves
   fresh and existing repos alike.
4. **A missing binary fails the commit loudly.** The hook is per-clone
   and deliberately installed; passing silently would re-introduce the
   drift it exists to prevent. The message names both fixes: reinstall
   workdown, or delete the hook.

Paths in the hook script (work items dir, `.workdown/`, render output
dir) are templated at install time from the loaded project config, not
hardcoded.

## Out of scope

- CI-side sync — that is [[github-render-action]].
