---
id: render-treemap
type: issue
status: to_do
title: Treemap renderer
parent: renderers
depends_on: [view-data-intermediate]
---

Render `TreemapView` as a Markdown file written to `views/<id>.md` — a hierarchical summary sized by a numeric field, grouped by a link field.

## Notes

- Hierarchy derived from the link field in `group:` (e.g. `group: parent`)
- Size field must be numeric; validated in `views-cross-file-validation`
- No native Markdown idiom for a true treemap — output is a data summary (nested bullet list with sizes is an obvious form)

## Acceptance

- `render_treemap(&TreemapView) -> String`
- Snapshot test with a two-level nested fixture
- Output renders correctly in GitHub preview
