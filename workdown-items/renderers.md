---
id: renderers
type: milestone
status: to_do
title: Renderers
parent: phase-04-visualization
depends_on: [foundation]
start_date: 2026-04-24
end_date: 2026-05-01
duration: "8d"
---

Produce rendered views from work items as lightweight Markdown files. One shared
`ViewData` intermediate feeds per-view-type renderers; each emits a single `.md`
file, using Mermaid code blocks where they express the shape well.

## Pipeline

```
items + views.yaml
      │
      ▼
 ViewData (shared, one variant per view type)
      │
      ├──► render_board      → views/<id>.md  (section-per-column)
      ├──► render_tree       → views/<id>.md  (nested bullet list)
      ├──► render_graph      → views/<id>.md  (mermaid flowchart)
      ├──► render_table      → views/<id>.md  (GFM table)
      ├──► render_gantt      → views/<id>.md  (mermaid gantt)
      ├──► render_bar_chart  → views/<id>.md  (mermaid xy-chart-beta or summary table)
      ├──► render_line_chart → views/<id>.md  (mermaid xy-chart-beta or summary table)
      ├──► render_workload   → views/<id>.md  (stacked table by date)
      ├──► render_metric     → views/<id>.md  (heading + bolded value)
      ├──► render_treemap    → views/<id>.md  (nested bullet list with sizes)
      └──► render_heatmap    → views/<id>.md  (GFM table grid)
         │
         ▼
    workdown render
```

## Goals

- Shared `ViewData` enum + extractors (one issue: `view-data-intermediate`)
- One issue per view type, each emitting Markdown
- `workdown render` — reads `views.yaml`, writes `views/<id>.md` per entry

## Notes

- Every view emits exactly one file per `views.yaml` entry: `views/<id>.md`
- Mermaid fenced blocks are used where they express the shape well (graph, gantt,
  bar/line charts); plain Markdown (tables, lists, sections) elsewhere
- The live server shares only the ViewData extraction layer with these renderers;
  the Svelte UI is an independent implementation per ADR-006
- Output form per view type is not configurable in v1 — each type has one form
- HTML output could be added later (e.g. standalone hosting outside GitHub); the
  Markdown pipeline doesn't preclude it
