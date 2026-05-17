---
id: multi-project-design
type: issue
status: to_do
parent: multi-project-support
title: Design multi-project support — set decisions and break out follow-up work
---

Turn the initial thoughts collected in [[multi-project-support]] into committed decisions, then spawn the implementation milestones and issues that follow.

## Goal

By the end of this issue:

- Every entry under "Initial thoughts" in [[multi-project-support]] is either confirmed (moved to a "Decisions" section) or revised based on what comes up while pinning it down.
- Every entry under "Open questions" in [[multi-project-support]] has a chosen answer with reasoning, or is explicitly deferred.
- Follow-up milestones and issues exist for the implementation work, with clear scope each.

## Decisions to confirm or revise

Each initial thought in [[multi-project-support]] should end up in one of three states:

- **Confirmed** — restate in the epic as a decision, move on.
- **Revised** — restate the new form and capture what changed and why.
- **Deferred** — note that this is intentionally unresolved for now, with a trigger for when to revisit.

The full list lives in the epic; not duplicating it here to avoid drift.

## Open questions to resolve

These are unsettled and need a chosen direction before implementation can start. Initial leanings noted where we have them — these are inputs to the discussion, not pre-baked answers.

### Schema mismatch escape hatch

Strict match only, or also allow a per-field mapping in the master workspace config?
**Initial leaning:** strict-only in v1. Mapping is conceptually appealing but tedious to author and breaks down when types diverge (choice vs integer). If real teams hit a blocker, add it later.

### Resource conflict tie-break when no master definition

Two sub-repos disagree on a field of the same resource id; master doesn't specify. Which wins?
**Initial leaning:** first by sub-repo list-order in `workspace.yaml` (not alphabetical), so the user controls precedence by ordering. Always emit a warning so the conflict is visible.

### Sub-repo identity across machines

Alias is the identity for workspace addressing — but how do tools verify a checkout at a given local path is actually the sub-repo the master expects?
**Initial leaning:** compare the local checkout's `origin` remote URL against the URL in `workspace.yaml` after normalization (HTTPS ↔ SSH, trailing `.git`). Mismatch → warning, not error.

### Web app server fetch authentication

The server needs to fetch sub-repos. Where do its credentials come from?
**Initial leaning:** delegate to the host environment — server uses whatever SSH agent / `.gitconfig` / credential helper is available. Don't invent a workdown-specific credential store.

### "Behind remote" indicator

How is "the local checkout is behind the tracked remote ref" detected, and how is it surfaced?
**Initial leaning:** periodic `git fetch` per sub-repo, then compare local HEAD against `origin/<branch>`. Surface in the web app as a per-sub-repo indicator with a "pull" hint. CLI output for cross-repo commands may also note it.

### Cross-repo aggregation semantics

Does the existing `aggregate` config cover rollups across the `tracks` link from a master item to sub-repo items, or is a new mechanism needed?
**Initial leaning:** extend the existing config to take an `over:` of any `links` field (currently it defaults to `parent`). A master item with `tracks: [...]` aggregates over `tracks` exactly the way an internal item aggregates over `parent`. No new field.

### Cross-repo addressing in CLI

`workdown set backend:auth-login status done` run from within a master — confirm the syntax and behavior.
**Initial leaning:** wherever an item id is accepted, accept `<alias>:<id>` too. Resolves through the workspace config. Bare ids in master continue to mean master-repo items. Sub-repos called without a checkout are read-only (write operations error with a clear message).

## Acceptance

- [[multi-project-support]]'s "Initial thoughts" section is replaced (or annotated) with confirmed decisions.
- [[multi-project-support]]'s "Open questions" section is empty or contains only intentionally-deferred items with documented triggers.
- Follow-up milestones exist under [[multi-project-support]] for the implementation work (likely: workspace config + loader, cross-repo CLI addressing, web app workspace mode, schema/resource merging, freshness indicator).
- Each follow-up milestone has at least a one-paragraph scope description; issues inside can come later.
