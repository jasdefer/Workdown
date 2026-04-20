---
id: render-tree
type: issue
status: to_do
title: Tree renderer
parent: renderers
depends_on: [view-data-intermediate]
---

Render `TreeView` as HTML, Markdown, and Mermaid.

## Output shapes

- **HTML** — nested `<details>`/`<summary>` for zero-JS expand/collapse; hydrated by the live server for richer interactivity
- **Markdown** — nested bulleted list, each node `[title](#id)`
- **Mermaid** — `graph TD` with parent→child edges (decide `mindmap` alternative during impl)

## Acceptance

- Three render functions
- Snapshot tests per format
- HTML expand/collapse works without JS
