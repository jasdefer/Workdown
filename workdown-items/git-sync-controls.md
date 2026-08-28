---
title: Git sync controls in the web UI
status: in_progress
---

Pull/push surface for teams whose item repo is shared, put where the
staleness problem lives: the board people already look at. Contributed
as PR #53; reviewed, rebased onto the 2026-08 maintenance milestone,
and reworked per the decisions below.

## What ships

- `GET /api/git` — tagged status (`disabled` / `not_a_repo` / `ready`
  with branch, ahead/behind, dirty count, and `fetch_error` when a
  requested remote contact failed). `?fetch=true` contacts the remote
  first and is origin-guarded like the mutations.
- `POST /api/git/pull` — fetch, then rebase; refuses over uncommitted
  changes or a rebase in progress; reports the integrated commit count.
- `POST /api/git/push` — publishes committed work only.
- A header pill (branch, `↓2 ↑1 · 3 local`, Pull/Push, a retryable
  "remote unreachable" hint), kept live by a server-side watch on the
  repository's git directory feeding a git-named SSE event.
- Opt-in via `serve.git_controls: true`; shells out to the user's own
  `git` so credential setups carry over.

## Decisions taken

1. **Pull refuses over uncommitted changes** (server 409 + button
   disabled with a "commit first" tooltip) instead of autostashing.
   A stash whose reapply conflicts exits 0 while scattering conflict
   markers into item files and hiding the edits in the stash — a
   browser button must not be able to end there. The tool's general
   answer to concurrent edits is git, so the answer here is "commit
   first".
2. **"Pulled N commits" is the behind count measured after the pull's
   own fetch** — the number the pill promised, and exactly what the
   rebase integrates. Measuring the tracking ref's movement during the
   pull undercounts to zero whenever an earlier status call already
   fetched (the primary flow), and can count a whole branch history
   when the tracking ref is missing.
3. **A status request degrades when only the remote is unreachable**
   (local numbers plus `fetch_error`) instead of failing whole. The
   pill stays useful offline and offers a manual retry; no automatic
   retry loop, so network activity stays tied to a user action.
4. **Terminal-side commits reach the pill via a server-side git watch**
   (`.git` head pointers and ref logs → git-named SSE event), not via
   window-focus listeners — those never fire in the board-on-second-
   monitor setup the feature exists for. One live-update channel per
   refresh scope, like the timer's. The index is deliberately not
   watched: the status endpoint's own `git status` may refresh it,
   which would loop.
5. **A rebase already in progress refuses the pull** (409) and is never
   aborted from the endpoint — it may be the user's own, half-resolved
   in a terminal. The endpoint only ever aborts a rebase it started,
   including after a timeout kills git mid-rebase.
6. **`?fetch=true` gets the same-origin guard and the git lock**: it
   invokes credential helpers and moves remote-tracking refs, so it is
   a side-effecting call in a read's clothing.

## Deferred

- A **Commit** action in the UI with a generated, editable message —
  flagged in the PR for design feedback before building.
- An origin guard for the other mutating endpoints (items, timer) —
  worth doing across the board; this PR guards only its own surface.
- Dogfooding: enable `serve.git_controls: true` in this repo's own
  `.workdown/config.yaml` only after the next release ships and
  binaries are updated — a 0.2.3 binary hard-fails on the unknown key
  (same release-ordering rule as `defaults.effort_field`, still
  pending for the same reason).
