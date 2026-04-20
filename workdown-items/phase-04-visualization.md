---
id: phase-04-visualization
type: epic
status: in_progress
title: "Phase 04: Visualization"
---

Build interactive and static visualization for workdown items.

## Deliverables

- Workspace split into `core` / `cli` / `server` crates with shared business logic
- Declarative `.workdown/views.yaml` for persisted views
- Three renderers (HTML, Markdown, Mermaid) fed by a shared `ViewData` intermediate
- `workdown render` — writes static views to disk (committable)
- `workdown serve` — local web app with board/tree/graph, drag-drop editing, SSE auto-update
- Svelte + TypeScript frontend embedded in the binary
- Runtime field selection in the live server (any compatible field can back a board/tree/graph)

## Principles

- **UI = ergonomic shell around CLI.** The web app exposes no capability the CLI lacks.
- **Single binary.** No extra runtime dependencies for users.
- **Repo is the source of truth.** Mutations write markdown files; user commits on their schedule.
- **No auto-commit.** CLI and UI mutations update the working tree only. Staging and committing stay a user action — never implicit.
- **Save with warning.** Schema violations surface as warnings, not hard rejects (ADR-001).

## Out of scope

- Time tracking / timers — parked for phase 05 if still wanted
- Rich markdown body editor — users have editors
- Remote / multi-user serve — local only
