---
id: render-command
type: issue
status: to_do
title: workdown render command
parent: renderers
depends_on:
  - view-data-intermediate
  - render-board
  - render-tree
  - render-graph
  - render-table
  - render-gantt
  - render-bar-chart
  - render-line-chart
  - render-workload
  - render-metric
  - render-treemap
  - render-heatmap
  - views-yaml-design
---

`workdown render` — reads `views.yaml`, produces static outputs per configured view.

## Behavior

- No args: render every view in `views.yaml`
- `workdown render <view-id>`: render just one
- For each view, determine the applicable output formats for its type, call the view-type renderer, write each format to `views/<id>.<ext>`
- Create the output directory if missing
- Idempotent: re-running with no item changes produces identical files (so CI diffs are clean)

## Acceptance

- Runs end-to-end against this repo's own work items
- Produces the expected outputs for each view type in `views.yaml`
- Paths are fixed per view id; no customization in v1
