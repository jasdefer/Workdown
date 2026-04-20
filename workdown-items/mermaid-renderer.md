---
id: mermaid-renderer
type: issue
status: to_do
title: Mermaid renderer
parent: renderers
depends_on: [view-data-intermediate]
---

Render `GraphView` and `TreeView` as Mermaid syntax blocks.

## Output shapes

- **GraphView**: `graph LR` with node IDs + edges
- **TreeView**: either `graph` (parent→child edges) or `mindmap` — decide during impl (mindmap is nicer visually, less universal)
- **BoardView**: not applicable (returns `None`)

## Acceptance

- `render_mermaid(&ViewData) -> Option<String>`
- Output renders in GitHub Markdown preview
- Node labels escape Mermaid-special characters safely
