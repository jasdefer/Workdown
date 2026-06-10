---
id: mutations-slice
title: Mutations end-to-end
type: issue
status: done
parent: server
depends_on:
- first-view-end-to-end
---

The mutation half of the UI. Every UI mutation maps to a `core` function — the same code path the CLI uses — wrapped in a thin HTTP handler. Save-with-warning surfaces in the UI.

## Scope

- `POST /api/items/:id/field` — body `{field, value, mode}`, calls `core::set_field` (modes follow `cli-set-modes`). Returns `{ok, warnings: [...]}`
- `POST /api/items` — create a new item, calls `core::add_item`
- Board drag-drop → `POST /api/items/:id/field` with the board field
- Item detail panel with per-field editing
- Read-only display of the item body (rendered Markdown; edits stay in the user's editor)
- Create-item form
- Warning display when the mutation response carries warnings

## Acceptance

- Drag a card between columns → markdown file on disk changes; no auto-commit
- Edit a field in the detail panel → file changes; warnings render if validation fails
- Create a new item via the form → new markdown file appears

## Out of scope

- Inline table editing — defer; revisit if useful
- Body editing in-browser
- Auto-commit
