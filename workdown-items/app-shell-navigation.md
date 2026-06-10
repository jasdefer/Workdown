---
id: app-shell-navigation
type: issue
status: done
title: App shell navigation (views menu + future link slots)
parent: server
depends_on: [first-view-end-to-end]
---

Add the navigation chrome the app shell needs once there's more than one place to be. Today's shell (per `ui-foundation`) is just header + theme toggle; the user navigates by typing URLs. Once multiple views exist, the user needs an in-app way to switch between them — and the chrome that holds those links is the same surface future non-view destinations (dynamic view generator, diagnostics, schema) will plug into.

## Scope

- View link list in the header: one entry per view from the layout-loaded `GET /api/views`. Label is `title` (or prettified id), kind label/icon hinted alongside.
- Active-view highlighting via `$page.url.pathname`.
- Inline horizontal layout, `flex-wrap` for natural multi-row when many views exist.
- Slot in the layout for non-view links (created empty here; populated by later issues — dynamic view generator, diagnostics, schema).
- Hidden entirely when `views.length === 0`.

## Acceptance

- Header renders a clickable view link for every entry in `views.yaml`.
- Clicking navigates to `/views/<id>` via SvelteKit client routing (no full page reload).
- Active view is visually distinct.
- With zero views configured, no navigation chrome renders.

## Out of scope

- Sidebar — header layout works for realistic view counts; promote only when a forcing function appears.
- Grouping views into folders or sections — defer until views.yaml supports it.
- Mobile / responsive design — local dev tool only.
- Search across views — not enough surface to warrant.
- "Create view" button — depends on dynamic view creation (separate future issue).
