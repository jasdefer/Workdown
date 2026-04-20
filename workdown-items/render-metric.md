---
id: render-metric
type: issue
status: to_do
title: Metric renderer
parent: renderers
depends_on: [view-data-intermediate]
---

Render `MetricView` — a single aggregated number with a label.

## Output shapes

- **HTML** — styled card: big number, label below. Inline CSS.
- **Markdown** — `**Label:** <number>` on a single line

## Notes

- Aggregates supported: `count`, `sum`, `avg`, `min`, `max`
- `count` needs no `value` slot; other aggregates require one
- The real payoff comes inside a future `dashboard` view (post-v1). Standalone metric files still have value on READMEs.

## Acceptance

- `render_metric_html(&MetricView) -> String`
- `render_metric_markdown(&MetricView) -> String`
- Snapshot tests for each aggregate
