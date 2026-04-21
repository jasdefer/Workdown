---
id: render-line-chart
type: issue
status: to_do
title: Line chart renderer
parent: renderers
depends_on: [view-data-intermediate]
---

Render `LineChartView` as HTML. Per-item scatter, connected by line when x is orderable (numeric or date).

## Output shapes

- **HTML** — SVG scatter / line chart, axis labels, point tooltips showing item id. Inline CSS.

## Notes

- x and y must be numeric or date fields (enforced in `views-cross-file-validation`)
- Items missing either field are dropped with a single aggregated warning
- No Mermaid output in v1. Mermaid's `xychart-beta` has a line variant but it's experimental and wastes scope on a second implementation.

## Acceptance

- `render_line_chart_html(&LineChartView) -> String`
- Snapshot test with a mixed-date/numeric fixture
