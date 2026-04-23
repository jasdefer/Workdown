---
id: render-graph
type: issue
status: to_do
title: Graph renderer
parent: renderers
depends_on: [view-data-intermediate]
---

Render `GraphView` as a Markdown file written to `views/<id>.md`, typically via a Mermaid `flowchart` block.

## Notes

- Mermaid `flowchart` is the natural fit — GitHub renders it inline
- Escape Mermaid-special characters in node labels

## Acceptance

- `render_graph(&GraphView) -> String`
- Snapshot test
- Output renders correctly in GitHub preview
