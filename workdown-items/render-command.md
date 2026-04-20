---
id: render-command
type: issue
status: to_do
title: workdown render command
parent: renderers
depends_on: [html-renderer, markdown-renderer, mermaid-renderer, views-yaml-design]
---

`workdown render` — reads `views.yaml`, produces static outputs per configured view.

## Behavior

- No args: render every view in `views.yaml`
- `workdown render <view-id>`: render just one
- For each view, iterate `output` formats (markdown, html, mermaid), call the appropriate renderer, write to the configured path
- Create output directories if missing
- Idempotent: re-running with no item changes produces identical files (so CI diffs are clean)

## Acceptance

- Runs end-to-end against this repo's own work items
- Produces the configured outputs (e.g. `views/board.md`, `views/graph.md`)
- Paths come from `views.yaml`, not hard-coded
