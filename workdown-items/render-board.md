---
id: render-board
type: issue
status: done
title: Board renderer
parent: renderers
depends_on: [view-data-intermediate]
---

Render `BoardView` as a Markdown file written to `views/<id>.md`.

## Notes

- One `##` heading per column value, cards under each as a bullet list is an obvious starting form — confirm during implementation
- Must render cleanly in GitHub preview and typical MD editors

## Acceptance

- `render_board(&BoardView) -> String`
- Snapshot test
- Output renders correctly in GitHub preview
