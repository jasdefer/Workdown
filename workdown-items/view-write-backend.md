---
id: view-write-backend
type: issue
status: to_do
title: Persist view definitions to views.yaml
parent: view-authoring
depends_on: []
effort: "16h"
---

The serve API can read views but cannot write them. The mutation work that landed for work items writes individual item files — it does not touch `views.yaml`. So there is currently no path for the UI (or any non-editor caller) to add a view or change a view's filter and have it persist.

This issue adds the ability to persist view definitions back to `views.yaml`: creating a new view, and adjusting an existing view's `where:` filter. It is the foundation both the filter editor and the view-creation menu build on. Like every other mutation in the tool, the repo stays the source of truth — changes update the working tree only, and the user commits when they choose.

## What we want

- A new view can be added to `views.yaml` from outside a text editor.
- An existing view's `where:` filter can be changed and persisted.
- A write that would produce an invalid `views.yaml` is reported back with the same diagnostics a hand-edited file would surface — the user is never left with a silently broken file.
- Writes update files only; nothing is staged or committed automatically.
- The persisted result reads cleanly — a human opening `views.yaml` afterwards sees a sensible, hand-editable file, not machine noise.

## Acceptance

- After creating a view through this path, the view appears in `views.yaml` and renders via the existing read endpoints.
- After changing a view's filter through this path, the new `where:` is reflected in `views.yaml` and in what the view shows.
- An invalid write returns diagnostics and does not leave `views.yaml` in a broken state.

## Out of scope

- Deleting, renaming, reordering, or fully re-slotting views — text-editor job for now (revisit if a UI need surfaces).
- Auto-commit / git integration.
- A UI — this is the persistence capability; [[view-filter-editor]] and [[view-creation]] consume it.
