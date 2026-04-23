---
id: render-metric
type: issue
status: to_do
title: Metric renderer
parent: renderers
depends_on: [view-data-intermediate]
---

Render `MetricView` as a Markdown file written to `views/<id>.md` — a single aggregated number with a label.

## Notes

- Aggregates supported: `count`, `sum`, `avg`, `min`, `max`
- `count` needs no `value` slot; other aggregates require one
- The real payoff comes inside a future `dashboard` view (post-v1); standalone metric files still have value when referenced from a README

## Acceptance

- `render_metric(&MetricView) -> String`
- Snapshot tests per aggregate
- Output renders correctly in GitHub preview
