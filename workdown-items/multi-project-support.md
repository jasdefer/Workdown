---
id: multi-project-support
type: epic
status: to_do
title: Multi-project support
---

Cross-repo aggregation and master-level planning for teams that work across several workdown repositories. A "master" repo with its own work items can reference items in any number of unaware sub-repos, supporting both personal cross-repo views ("my day") and umbrella initiatives spanning multiple projects.

The contents below are the outcome of an initial brainstorm. Treat them as **initial thoughts, not set in stone** — [[multi-project-design]] is responsible for sharpening them into committed decisions and breaking out follow-up implementation work.

## Use cases

1. **Personal cross-repo planning** — one person, several repos, "what's on my plate across all of them today."
2. **Team-wide board / standup** — a team lead or PM sees all items across the repos the team owns.
3. **Master initiative spanning sub-projects** — "Ship new auth" lives at the master level; the work items it tracks live in 5 different repos.
4. **Cross-repo progress aggregation** — `% done`, person-days remaining, etc. rolled up from sub-repo leaves to a master umbrella.
5. **Shared resources** — people, teams, sprints used by multiple repos without redefining them each time.
6. **Rollup reporting** — "what did Sprint 23 ship across all team repos."
7. **Cross-repo refactor coordination** — short-lived master initiative tracking sub-tasks in many repos for a migration.

## Initial thoughts (not set in stone)

### Architecture

- **Sub-repos are unaware.** A workdown repo can be aggregated by any number of masters without knowing it. No special configuration in the sub-repo.
- **The master is a normal workdown repo** with its own items, schema, resources, plus extra workspace configuration. Any repo *can* be a master by adding that configuration — it isn't a separate concept.
- **No git submodules or subtrees.** Sub-repos are independent clones; the master config references them by identity.
- **One level of nesting only** in the first version: a sub-repo cannot itself be a master with further sub-sub-repos.

### Workspace configuration (two files)

- `.workdown/workspace.yaml` — **committed**. Lists each sub-repo's identity: alias, git URL, default branch.
- `.workdown/workspace.local.yaml` — **gitignored**. Per-user local checkout paths for the sub-repos the user has cloned.

### Read vs write paths

- **Reads** (views, queries, web app) do not require a local checkout. `git fetch` followed by reading blobs from the remote ref is enough. Users who only consume cross-repo views never need to clone sub-repos.
- **Writes** (CLI edits to a sub-repo item from the master) require a local checkout. The CLI edits the file at the known local path; the user is responsible for committing and pushing in that sub-repo.
- **The CLI and web app never commit or push.** Mutations are working-tree edits only. The user controls when changes land in git.

### Addressing and references

- Cross-repo references use `project_alias:item_id` (e.g. `backend:auth-login`).
- The alias is the identity for workspace purposes; the URL is bootstrapping help. Two users with different remote configurations (HTTPS vs SSH) can still agree on the alias.
- Master-level items that span sub-repos use a regular `links`-type field (e.g. `tracks: [backend:auth-login, frontend:login-page]`) to reference sub-repo items. No separate overlay file format — extra fields live on the master item.

### Schema compatibility across repos

- **Strict match by default.** A field is available in cross-repo views only if its definition (type, and for `choice` types its value set) matches across all participating repos.
- Non-matching fields stay usable per-repo but are simply unavailable for cross-repo aggregation, filtering, or board grouping.
- Whether to offer a per-field mapping escape hatch in the master config (`backend.priority.low → P2`) is still open — see [[multi-project-design]].

### Resources

- Same `id` across repos = same entity. Resource definitions merge field-by-field.
- Master definitions override sub-repo definitions for any field the master specifies.
- If no master definition exists and sub-repos disagree on a field, the workspace warns and picks deterministically; exact tie-break order is still open.

### Freshness

- Auto-fetch is fine and expected. Auto-pull never happens — the user may have local work.
- When a sub-repo's local checkout is behind its tracked remote ref, the web app surfaces a "behind remote" indicator so the user can pull deliberately.

## Open questions

- Schema mismatch escape hatch — strict-only, or allow per-field mapping?
- Resource conflict tie-break when no master definition — alphabetical-by-alias, list-order, something else?
- Sub-repo identity across machines — canonical URL, alias-only, normalized remote, or all of the above?
- Web app server auth model for fetching sub-repos (SSH key, PAT, delegated to host environment).
- How "behind remote" is detected and surfaced.
- Cross-repo aggregation semantics — does the existing `aggregate` config cover rollups across repos, or is a new mechanism needed for `tracks`-style links?
- CLI ergonomics for cross-repo addressing (`workdown set backend:auth-login status done` from within the master) — confirm the parser change and error messages.

## Out of scope (for now)

- Cross-repo transactional edits. Edits across several sub-repos are not atomic; the user reconciles via separate commits per repo.
- Cross-repo blocking / dependency enforcement (e.g. preventing a sub-repo merge because a master item says "blocked"). May be revisited later.
- Nested workspaces (a sub-repo being a master itself).
- Server-mediated authentication beyond reading files at a given ref.
