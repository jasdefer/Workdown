---
id: render-gantt
type: issue
status: to_do
title: Gantt renderer
parent: renderers
depends_on: [view-data-intermediate]
---

Render `GanttView` as Mermaid and HTML. Markdown reuses Mermaid inside a fenced block.

## Output shapes

- **Mermaid** — `gantt` syntax block. Sections per `group` field value if configured; otherwise one flat section.
- **HTML** — embeds the Mermaid output in `<pre class="mermaid">` with `mermaid.js`. Avoids a second gantt implementation.
- **Markdown** — ```` ```mermaid ```` fenced block

## Notes

- Items missing `start` or `end` are dropped with a single aggregated warning
- Date parsing reuses the existing `date` field-type validation

## Acceptance

- Three render functions
- Mermaid output renders in GitHub preview
- HTML opens from `file://` and displays correctly
