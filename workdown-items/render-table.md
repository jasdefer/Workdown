---
id: render-table
type: issue
status: to_do
title: Table renderer
parent: renderers
depends_on: [view-data-intermediate]
---

Render `TableView` as a Markdown file written to `views/<id>.md`.

## Notes

- GFM table is the natural form
- Escape pipe characters in field values; empty values render as blanks

## Acceptance

- `render_table(&TableView) -> String`
- Snapshot test
- Output renders correctly in GitHub preview
