---
id: render-table
type: issue
status: to_do
title: Table renderer
parent: renderers
depends_on: [view-data-intermediate]
---

Render `TableView` as HTML and Markdown.

## Output shapes

- **HTML** — `<table>` with `<thead>` + `<tbody>`. Column order from `views.yaml`. Sortable via JS in the live server; static output is plain.
- **Markdown** — GitHub-flavored table (pipe-delimited, aligned). Pipe characters in field values escaped. Empty values render as blanks.

## Acceptance

- `render_table_html(&TableView) -> String`
- `render_table_markdown(&TableView) -> String`
- Snapshot tests for both
- MD output renders correctly in GitHub preview
