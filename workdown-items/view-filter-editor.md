---
id: view-filter-editor
type: issue
status: to_do
title: Build and edit a view's where filter from the UI
parent: view-authoring
depends_on: [remaining-read-views, schema-metadata-api, view-write-backend]
effort: "16h"
---

A view's `where:` clauses are static in `views.yaml` — adjusting what a view shows means editing YAML and reloading. Once users routinely want "items I'm working on this week", "items for team X", and so on, they need to narrow a view from the app.

This issue delivers a filter-building experience that does double duty, and is the reusable piece [[view-creation]] composes a new view's filter with — built once here, used in both places.

## What we want

- From any view, a user can add, change, and remove filter conditions and see the view re-narrow immediately.
- Conditions are built against the schema, not free text: the field, the operator, and (where the field constrains them) the value are chosen from what's valid, drawing on [[schema-metadata-api]]. An escape hatch for expressing a raw condition covers anything the guided builder doesn't.
- Two ways to keep a filter, both supported:
  - **For right now** — a personal, throwaway narrowing that doesn't change `views.yaml` and isn't shared.
  - **Saved** — written back into the view's `where:` in `views.yaml` via [[view-write-backend]], so it persists and is shared with the project.
- Whatever the builder produces means exactly what the same expression would mean when typed into `views.yaml` by hand — one filter grammar, one behavior.

## Acceptance

- A user can narrow any view without touching `views.yaml`, and choose whether that narrowing is just-for-now or saved into the view.
- A saved filter shows up in `views.yaml` and survives a reload; a for-now filter does not alter the file.
- A filter built in the UI and the same filter hand-written in `views.yaml` produce identical results.

## Out of scope

- A full SQL-like query builder — the guided builder plus a raw escape hatch covers the common cases; arbitrarily complex predicates remain a text-editor job.
- A saved-filter library / shared presets beyond a view's own `where:` — defer.
- Per-view display configuration (which fields show where) — that's [[view-display-config]].
