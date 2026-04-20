---
id: server-endpoints-and-mutations
type: issue
status: to_do
title: Query and mutation endpoints
parent: server
depends_on: [serve-command-skeleton]
---

Add the JSON API the frontend calls. All handlers are thin wrappers over `core`.

## Endpoints (initial sketch — refine during implementation)

- `GET /api/views` — list configured views from `views.yaml`
- `GET /api/views/:id` — return `ViewData` as JSON for a named view
- `GET /api/views/runtime?type=board&field=status` — render a view from runtime params (supports "pick any compatible field")
- `GET /api/items` — list all items (id, title, type, status)
- `GET /api/items/:id` — full item data
- `POST /api/items/:id/field` — body `{field, value}`, calls `core::set_field`. Returns `{ok, warnings: [...]}` per save-with-warning
- `POST /api/items` — create a new item, calls `core::add_item`

## Acceptance

- Each handler is a ~10-line thin wrapper over `core`
- Integration tests hit the endpoints against a temp workdown project
- Errors surface as well-formed JSON with status codes
