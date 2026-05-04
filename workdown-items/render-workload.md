---
id: render-workload
type: issue
status: done
title: Workload renderer
parent: renderers
depends_on: [view-data-intermediate]
---

Render `WorkloadView` as a Markdown file written to `views/<id>.md`. Items distribute their `effort` uniformly across `start`→`end`, summed by date bucket.

## Notes

- Uniform distribution only in v1 (documented limitation)
- Day-level time resolution — one bucket per calendar day
- Items missing `start`, `end`, or `effort` are dropped with a single aggregated warning
- A stacked table by date bucket is an obvious starting form — decide at implementation

## Acceptance

- `render_workload(&WorkloadView) -> String`
- Snapshot test with overlapping intervals
- Output renders correctly in GitHub preview
