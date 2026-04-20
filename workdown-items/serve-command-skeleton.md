---
id: serve-command-skeleton
type: issue
status: to_do
title: workdown serve skeleton
parent: server
---

Get `workdown serve` to start an axum web server. No embedded assets yet, no API — just proves the command works and a browser can reach it.

## Scope

- `workdown serve [--port N] [--open]` CLI command (defaults: auto-port, no browser open)
- axum listening on localhost, returning a hardcoded `"workdown serve — not yet implemented"` from `GET /`
- Startup log line: URL, port, PID

## Out of scope

- Embedded frontend assets (next issue: `ui-build-integration`)
- API endpoints
- TLS, auth — local only
