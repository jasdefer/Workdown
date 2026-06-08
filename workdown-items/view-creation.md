---
id: view-creation
type: issue
status: to_do
title: Create a new view from the UI
parent: view-authoring
depends_on: [view-write-backend, schema-metadata-api, view-filter-editor, app-shell-navigation]
effort: "16h"
---

A user who wants a new view has to leave the app, learn the `views.yaml` shape for the kind they want, and hand-write it. This issue lets them assemble a view in the browser instead: choose a kind, fill in what that kind needs, optionally attach a filter, and save it.

This is the "Create view" entry point that [[app-shell-navigation]] deliberately deferred. The filter portion reuses the builder from [[view-filter-editor]] rather than reinventing it, and saving goes through [[view-write-backend]].

## What we want

- A single place in the UI to compose a new view: pick the kind, then supply the inputs that kind requires (driven by the schema metadata, so only valid fields are offered).
- A filter can be attached during creation using the same builder used to narrow existing views.
- Before saving, the user can tell whether the view is valid — a misconfigured view is caught here, not after it's written.
- On save, the view is written to `views.yaml`, appears in the navigation, and renders.
- Reachable from the navigation chrome, which currently has a slot waiting for it.

## Acceptance

- A user can go from "I want a board grouped by X" to a rendered, navigable view without editing `views.yaml` directly.
- The created view is a normal `views.yaml` entry — indistinguishable from a hand-written one and editable as such.
- Required inputs missing or incompatible for the chosen kind are surfaced before the view is saved.

## Out of scope

- Editing an existing view's kind or slots after creation — text-editor job (this issue only *creates*; filter changes live in [[view-filter-editor]]).
- Duplicating views, templates, or a view gallery — defer until the need is real.
- Per-view display configuration — that's [[view-display-config]].
