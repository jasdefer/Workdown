---
id: renderers
type: milestone
status: to_do
title: Renderers
parent: phase-04-visualization
depends_on: [foundation]
---

Produce rendered views from work items. One shared `ViewData` intermediate feeds per-view-type renderers, each emitting one or more of HTML / Markdown / Mermaid.

## Pipeline

```
items + views.yaml
      │
      ▼
 ViewData (shared, one variant per view type)
      │
      ├──► render_board      (html + md)
      ├──► render_tree       (html + md + mermaid)
      ├──► render_graph      (html + mermaid)
      ├──► render_table      (html + md)
      ├──► render_gantt      (mermaid + html)
      ├──► render_bar_chart  (html + mermaid)
      ├──► render_line_chart (html)
      ├──► render_workload   (html)
      ├──► render_metric     (html + md)
      ├──► render_treemap    (html)
      └──► render_heatmap    (html)
         │
         ▼
    workdown render
```

## Goals

- Shared `ViewData` enum + extractors (one issue)
- One issue per view type, each covering all applicable output formats for that type
- `workdown render` — ties it together, writes files per `views.yaml`

## Notes

- Markdown output for graph / gantt / bar_chart is the mermaid output wrapped in a ```` ```mermaid ```` fence — not a separate renderer
- The live server reuses these same renderers; UI hydration layers on top
- Formats per view type are not configurable in v1 — each type writes a fixed set of output files at `views/<id>.<ext>`
