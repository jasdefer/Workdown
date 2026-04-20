---
id: render-board
type: issue
status: to_do
title: Board renderer
parent: renderers
depends_on: [view-data-intermediate]
---

Render `BoardView` as HTML and Markdown.

## Output shapes

- **HTML** — one `<section>` per column, cards are `<article>` elements carrying `data-item-id` + `data-field`. Inline CSS. Hydration hooks for the live server to attach drag-drop.
- **Markdown** — GFM table: columns = board field values, rows align cards into cells. Fallback to section-per-column when the table gets too wide.

## Acceptance

- `render_board_html(&BoardView) -> String`
- `render_board_markdown(&BoardView) -> String`
- Snapshot tests for both
- HTML opens from `file://` and displays correctly; MD renders in GitHub preview
