---
id: server-sse-file-watching
type: issue
status: to_do
title: File watcher and SSE for auto-update
parent: server
depends_on: [server-endpoints-and-mutations]
---

Push updates to connected browsers when work item files change on disk (editor save, git pull, CLI mutation).

## Scope

- Watch `workdown-items/` and `.workdown/` using the `notify` crate
- Debounce rapid changes (vim's atomic save often looks like delete+create)
- Expose `GET /api/events` as a Server-Sent Events stream
- On file change, push a typed event (`item_changed`, `schema_changed`, `views_changed`) with enough metadata for the client to refetch the right data
- Clean up subscriptions on client disconnect (no leaks)

## Out of scope

- WebSockets — SSE is sufficient for one-way server→client push
- Reconnection logic — browsers handle `EventSource` reconnect automatically; revisit only if that's inadequate
