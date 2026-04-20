---
id: markdown-renderer
type: issue
status: to_do
title: Markdown renderer
parent: renderers
depends_on: [view-data-intermediate]
---

Render `ViewData` to GitHub-flavored Markdown.

## Format per view type

- **Board**: a table — columns = board field values, rows = card titles. Fallback to a section-per-column if tables get too wide.
- **Tree**: nested bulleted list with `id` links
- **Graph**: a ` ```mermaid ` code block (delegated to the Mermaid renderer)

## Acceptance

- `render_markdown(&ViewData) -> String`
- Snapshot tests
- Output renders correctly in GitHub's preview
