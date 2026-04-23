---
id: render-line-chart
type: issue
status: to_do
title: Line chart renderer
parent: renderers
depends_on: [view-data-intermediate]
---

Render `LineChartView` as a Markdown file written to `views/<id>.md`.

## Notes

- x and y must be numeric or date fields (enforced in `views-cross-file-validation`)
- Items missing either field are dropped with a single aggregated warning
- Form options: Mermaid `xychart-beta` line variant (experimental) or a plain summary table — decide at implementation

## Acceptance

- `render_line_chart(&LineChartView) -> String`
- Snapshot test
- Output renders correctly in GitHub preview
