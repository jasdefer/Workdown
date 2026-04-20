---
id: render-bar-chart
type: issue
status: to_do
title: Bar chart renderer
parent: renderers
depends_on: [view-data-intermediate]
---

Render `BarChartView` as HTML and Mermaid. Markdown reuses Mermaid inside a fenced block.

## Output shapes

- **HTML** — SVG bars, axis labels, value labels on hover. Inline CSS.
- **Mermaid** — `xychart-beta` bar variant. Verify rendering in GitHub Markdown preview before declaring done — `xychart-beta` is marked experimental.
- **Markdown** — ```` ```mermaid ```` fenced block

## Notes

- Aggregates supported: `sum`, `count`, `avg`, `min`, `max`
- Items are filtered by `where` before aggregation (handled in the extractor)

## Acceptance

- Three render functions
- Snapshot tests for HTML
- Mermaid output renders in GitHub preview (otherwise document the limitation and ship HTML only)
