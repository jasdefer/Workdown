---
id: render-treemap
type: issue
status: to_do
title: Treemap renderer
parent: renderers
depends_on: [view-data-intermediate]
---

Render `TreemapView` — hierarchical boxes sized by a numeric field, grouped by a link field (typically `parent`).

## Output shapes

- **HTML** — SVG squarified treemap. Rectangles labeled with item title + size. Inline CSS.

## Notes

- Hierarchy derived from the link field in `group:` (e.g. `group: parent`)
- Size field must be numeric; validated in `views-yaml-validation`
- Squarified treemap algorithm is well-documented; implement directly or pull a small crate — decide during impl
- No Markdown or Mermaid output — neither format has useful support

## Acceptance

- `render_treemap_html(&TreemapView) -> String`
- Snapshot test with a two-level nested fixture
