---
id: first-view-end-to-end
type: issue
status: to_do
title: First view end-to-end (board, read-only)
parent: server
depends_on: [walking-skeleton, ui-foundation]
---

Wire one view all the way through the stack. Board is the pick: most visually distinct, lowest risk of looking trivial. This slice is where the API JSON shape, error envelope, warning surface, and Svelte fetch/component patterns get decided — against a real consumer, in one PR.

## Scope

- `GET /api/views` — list configured views from `views.yaml`
- `GET /api/views/:id` — return `ViewData` as JSON for a board view
- Svelte board component renders columns and cards from the JSON
- Fetch layer + error/loading state pattern established for later slices
- API conventions decided here (code review is sufficient — no separate ADR; internal contract):
  - JSON envelope shape
  - Error format and HTTP status codes
  - How `ViewData` serializes (especially typed field values)

## Acceptance

- `workdown serve` → browser shows a real board with real data
- Integration tests hit the endpoints against a temp workdown project
- Conventions land as type definitions / module comments, not standalone prose docs

## Out of scope

- Other view types (next slice)
- Mutations (board drag-drop comes with the mutations slice)
- Runtime field selection — defer; revisit when a UI need surfaces
