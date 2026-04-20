---
id: renderers
type: milestone
status: to_do
title: Renderers
parent: phase-04-visualization
depends_on: [foundation]
---

Produce rendered views from work items in three output formats, sharing an intermediate `ViewData` structure.

## Pipeline

```
items + views.yaml
      │
      ▼
 ViewData (shared)
      │
      ├──► HtmlRenderer    (static + hydratable by the live server)
      ├──► MarkdownRenderer (GitHub-native tables / lists)
      └──► MermaidRenderer  (graph syntax inside Markdown)
```

## Goals

- `ViewData` enum (one variant per view type: board, tree, graph) + extractors
- Three renderer adapters consuming `ViewData`
- `workdown render` — ties it together, writes files per `views.yaml`
