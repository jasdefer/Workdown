---
id: git-derived-default-generator
type: issue
status: to_do
title: Default generator that reads dates from git history
parent: time-tracking
---

Tracking when an item actually started or finished is manual today.
Users forget. But the repo already records the truth — git knows when
frontmatter changed and when. The idea: a default-value generator that
resolves to the date a field first (or last) took a given value, read
from git history.

Two configurations of this generator give actual-start and
actual-completion stamps (e.g. status → `in_progress`, status → `done`).
Users override by writing a value into frontmatter directly — that's
how defaults already behave, no new override mechanism needed.

## Why this fits workdown

- Repo as source of truth is already a guiding principle.
- Reuses the existing default-generator concept (`$today`, `$uuid`, …).
- Snapshot-only validation (ADR-001) still holds — this changes how
  unset defaults are computed, not how state is validated.

## Open questions

- Slot into the existing default-generator system, or treat as a
  separate "computed from git" mechanism? Override semantics differ.
- File renames — `git log --follow` is the obvious choice; ambiguous
  rename history needs a position.
- Author-date vs commit-date — rebases and amends behave differently.
- First match vs last match — both useful. The right default may depend
  on the field.
- Behavior when the value never appeared in history.
- Behavior with uncommitted edits that match the condition (probably
  ignored, consistent with "git is the truth", but confirm).
- Performance — `git log --follow -- <file>` per item per render.
  Acceptable at what scale? Caching is a deferred follow-on, not in
  scope here.
- Possibly an ADR — this is the first default that reads state outside
  its own file.

## Dogfooding

Once shipped, add actual-start and actual-completion stamps to this
repo's schema wired to status transitions.
