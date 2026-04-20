---
id: html-renderer
type: issue
status: to_do
title: HTML renderer
parent: renderers
depends_on: [view-data-intermediate]
---

Render `ViewData` to self-contained HTML.

## Requirements

- Same template produces static output and live-server output — live server hydrates with JS for drag-drop; static version is read-only
- Inline CSS (no external asset URLs) so outputs are hostable anywhere
- Graph view uses `mermaid.js` (loaded from CDN or embedded — decide during impl)
- Valid HTML5, no external runtime deps when served statically
- Basic accessibility: headings, landmarks, focusable cards

## Acceptance

- `render_html(&ViewData) -> String` for all three view types
- Snapshot test per view type
- Rendered page opens in a browser from `file://` and displays correctly
