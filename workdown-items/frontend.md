---
id: frontend
type: milestone
status: to_do
title: Frontend
parent: phase-04-visualization
depends_on: [server]
---

Svelte + TypeScript UI, built with Vite, embedded into the workdown binary via `rust-embed`. Runs in the browser when `workdown serve` is active.

**Decomposition deferred** until the `server` milestone lands — the frontend contract depends on what the API looks like in reality.

## Known scope (for context, not as issues yet)

- Board view with drag-drop → `POST /api/items/:id/field`
- Tree view (read-only, click to expand)
- Graph view (Mermaid v1; Cytoscape if interactive graph editing becomes a priority)
- Item detail panel with field editing
- Read-only rendering of the markdown body inside the detail panel (body edits happen in the user's editor)
- Create-item form
- SSE subscription for auto-update
- Validation-warning display when save-with-warning returns warnings

## Known open questions

- Graph library for the live view: Mermaid vs Cytoscape. Decide when we get here.
- Card content: what fields show on a card by default? Per-view config?
- Dark mode / theming: in scope for v1 or defer?
