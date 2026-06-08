---
id: live-updates
type: issue
status: to_do
title: File watcher and SSE for live updates
parent: server
depends_on: [walking-skeleton]
---

Push updates to connected browsers when work item files change on disk (editor save, git pull, CLI mutation, server mutation). Independent of the read and mutation slices — can land any time after the skeleton, though most demoable once at least one view exists.

## Scope

- Watch `workdown-items/` and `.workdown/` using the `notify` crate
- Debounce rapid changes (vim's atomic save often looks like delete+create)
- `GET /api/events` Server-Sent Events stream
- Typed events (`item_changed`, `schema_changed`, `views_changed`) with enough metadata for the client to refetch the right data
- Svelte subscription: on event, refetch the affected view
- Clean up subscriptions on client disconnect (no leaks)

## Out of scope

- WebSockets — SSE is sufficient for one-way server → client push
- Custom reconnection logic — browser `EventSource` handles it; revisit only if inadequate
