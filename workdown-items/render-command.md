---
id: render-command
type: issue
status: to_do
title: workdown render command
parent: renderers
depends_on:
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
---

`workdown render` — reads `views.yaml`, produces static outputs per configured view.

## Behavior

- No args: render every view in `views.yaml`
- `workdown render <view-id>`: render just one
- For each view, call the view-type renderer and write the returned Markdown to `views/<id>.md`
- Create the output directory if missing
- Idempotent: re-running with no item changes produces identical files (so CI diffs are clean)

## Acceptance

- Runs end-to-end against this repo's own work items
- Produces one `.md` file per entry in `views.yaml`
- Paths are fixed: `views/<id>.md`; no customization in v1
