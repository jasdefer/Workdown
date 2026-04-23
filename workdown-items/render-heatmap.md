---
id: render-heatmap
type: issue
status: to_do
title: Heatmap renderer
parent: renderers
depends_on: [view-data-intermediate]
---

Render `HeatmapView` as a Markdown file written to `views/<id>.md` — a 2D grid of aggregated values.

## Notes

- x and y axes accept choice, string, or date fields
- Date axes support a `bucket: day | week | month` slot in the view config — extractor buckets dates before aggregating
- Aggregate a numeric field, or count items when `value` is omitted
- No native Markdown idiom for a heatmap with colour — a GFM table grid is an obvious form

## Acceptance

- `render_heatmap(&HeatmapView) -> String`
- Snapshot test with one categorical axis + one date axis bucketed by week
- Output renders correctly in GitHub preview
