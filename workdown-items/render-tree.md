---
id: render-tree
type: issue
status: done
title: Tree renderer
parent: renderers
depends_on: [view-data-intermediate]
---

Render `TreeView` as a Markdown file written to `views/<id>.md`.

## Notes

- Nested bullet list is the natural form
- Link each node to its source work item (e.g. `[title](../workdown-items/<id>.md)`) — decide path convention at implementation

## Acceptance

- `render_tree(&TreeView) -> String`
- Snapshot test
- Output renders correctly in GitHub preview
