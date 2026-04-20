---
id: render-workload
type: issue
status: to_do
title: Workload renderer
parent: renderers
depends_on: [view-data-intermediate]
---

Render `WorkloadView` — items distribute their `effort` uniformly across `start`→`end`, summed daily to produce a staffing-demand curve.

## Output shapes

- **HTML** — SVG area chart. Time axis, value axis labeled "effort / day" (or the unit derived from the schema field). Inline CSS.

## Notes

- Uniform distribution only in v1 (documented limitation)
- Day-level time resolution — one bucket per calendar day
- Items missing `start`, `end`, or `effort` are dropped with a single aggregated warning
- No Markdown or Mermaid output — neither format represents the curve usefully

## Acceptance

- `render_workload_html(&WorkloadView) -> String`
- Snapshot test
- Verified visually against a small fixture with overlapping intervals
