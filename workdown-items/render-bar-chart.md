---
id: render-bar-chart
type: issue
status: done
title: Bar chart renderer
parent: renderers
depends_on: [view-data-intermediate]
---

Render `BarChartView` as a Markdown file written to `views/<id>.md`.

## Notes

- Aggregates supported: `sum`, `count`, `avg`, `min`, `max`
- Items are filtered by `where` before aggregation (handled in the extractor)
- Form options: Mermaid `xychart-beta` bar variant (experimental — verify GitHub support) or a plain summary table — decide at implementation

## Acceptance

- `render_bar_chart(&BarChartView) -> String`
- Snapshot test
- Output renders correctly in GitHub preview
