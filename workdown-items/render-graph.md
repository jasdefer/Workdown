---
id: render-graph
type: issue
status: to_do
title: Graph renderer
parent: renderers
depends_on: [view-data-intermediate]
---

Render `GraphView` as HTML and Mermaid. Markdown reuses the Mermaid output inside a fenced block.

## Output shapes

- **Mermaid** — `graph LR` with nodes + directed edges. Node labels escape Mermaid-special characters.
- **HTML** — embeds the Mermaid output inside `<pre class="mermaid">`; `mermaid.js` renders it on load (bundled, not CDN)
- **Markdown** — ```` ```mermaid ```` fenced block wrapping the mermaid output — produced as a trivial wrapper, not a separate renderer

## Acceptance

- `render_graph_mermaid(&GraphView) -> String`
- `render_graph_html(&GraphView) -> String`
- `render_graph_markdown(&GraphView) -> String` (fence wrapper)
- Mermaid output renders in GitHub preview
