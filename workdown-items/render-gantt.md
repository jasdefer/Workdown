---
id: render-gantt
type: issue
status: to_do
title: Gantt renderer
parent: renderers
depends_on: [view-data-intermediate]
---

Render `GanttView` as a Markdown file written to `views/<id>.md`, typically via a Mermaid `gantt` block.

## Notes

- Sections per `group` field value if configured; otherwise one flat section
- Items missing `start` or `end` are dropped with a single aggregated warning
- Date parsing reuses the existing `date` field-type validation

## Acceptance

- `render_gantt(&GanttView) -> String`
- Snapshot test
- Output renders correctly in GitHub preview
