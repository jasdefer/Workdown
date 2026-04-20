---
id: render-heatmap
type: issue
status: to_do
title: Heatmap renderer
parent: renderers
depends_on: [view-data-intermediate]
---

Render `HeatmapView` — 2D grid of aggregated values, color intensity encodes the number.

## Output shapes

- **HTML** — SVG or div-grid heatmap. Axis labels, color-scale legend. Inline CSS.

## Notes

- x and y axes accept choice, string, or date fields
- Date axes support a `bucket: day | week | month` slot in the view config — extractor buckets dates before aggregating
- Aggregate a numeric field, or count items when `value` is omitted
- No Markdown (loses the color channel) or Mermaid (no heatmap support) output

## Acceptance

- `render_heatmap_html(&HeatmapView) -> String`
- Snapshot test with one categorical axis + one date axis bucketed by week
