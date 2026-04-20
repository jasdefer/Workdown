---
id: adr-phase-04-architecture
type: issue
status: done
title: ADR — visualization architecture
parent: foundation
---

File an ADR in `docs/adr/006-visualization-architecture.md` capturing the decisions agreed at the start of phase 04, so they're recorded before implementation drifts. Should be the first piece of work in `foundation`.

## Decisions to document

- **Workspace layout** — three crates: `core` (pure library), `cli` (thin binary), `server` (axum library). Both `cli` and `server` call `core` directly via function calls, not HTTP or shell-out.
- **Single binary** — frontend (Svelte + TS) compiled by Vite, embedded in the CLI binary via `rust-embed`. User installs one binary; no node, no .NET, no Python required at runtime.
- **UI = ergonomic shell around CLI** — every UI mutation maps 1:1 to a CLI subcommand. If you can't `workdown X` from the terminal, you can't do it in the UI.
- **Shared `ViewData` with renderer adapters** — one extraction pipeline produces an intermediate representation; HTML, Markdown, and Mermaid renderers consume it.
- **HTML live + static from the same template** — progressive enhancement; JS hydrates drag-drop on top of valid static HTML.
- **`views.yaml` for persisted views; runtime field selection on live server** — the config declares what we render and commit; the server lets users explore any compatible field on the fly.
- **Save-with-warning on mutations** — schema violations emit warnings; the mutation still succeeds (consistent with ADR-001's snapshot validation philosophy).
- **No auto-commit** — mutations update the working tree only. Staging and committing are user actions, never implicit.

## Scope

- Write `docs/adr/006-visualization-architecture.md`, slim per the project convention (Status / Date / Context / Decision / Consequences)

## Acceptance

- ADR committed
- Readable in under two minutes
- Each decision above appears in the Decision section with one sentence of reasoning
