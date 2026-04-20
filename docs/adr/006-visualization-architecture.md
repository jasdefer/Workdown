# ADR-006: Visualization architecture

**Status:** Accepted
**Date:** 2026-04-20

## Context

Phase 04 adds visualization to workdown: static rendered views (`workdown render`) and an interactive local web UI (`workdown serve`). This ADR captures the structural decisions before implementation begins.

## Decisions

### Workspace layout

The single crate splits into a Cargo workspace with three crates:

- `core` — pure library: parsing, validation, querying, mutation, schema loading
- `cli` — thin binary wrapping `core` via clap subcommands
- `server` — axum-based local web server, also calling `core` directly

Both `cli` and `server` depend on `core` and call it via normal Rust function calls — no shell-out, no HTTP between them.

### Single binary

The Svelte + TypeScript frontend is compiled by Vite at build time and embedded in the CLI binary via `rust-embed`. Users install one binary with no runtime dependencies (no Node, no Python, no .NET).

### UI is a shell around the CLI

Every mutation the web UI can perform maps 1:1 to a CLI subcommand. The server calls the same `core` functions the CLI does. If something can't be done with `workdown <subcommand>`, it can't be done in the UI.

### Shared ViewData intermediate

A single extraction pipeline reads work items and produces a `ViewData` structure (board columns with cards, tree nodes, graph edges). ViewData is fully resolved — grouping, sorting, and field lookup happen once during extraction.

Renderers consume ViewData and are pure formatting: they translate the resolved structure into a specific output format. No business logic lives in renderers.

### Static and interactive are separate

`workdown render` produces self-contained static files (HTML, Markdown, Mermaid) meant to be committed to the repo — readable on GitHub, usable in CI.

`workdown serve` runs a Svelte SPA backed by JSON API endpoints with SSE-based live updates — a local development tool.

The two share the ViewData extraction layer (same grouping, sorting, and resolution logic) but not templates or rendering code. Static HTML and the Svelte UI are independent implementations with different constraints: one optimizes for portability and committability, the other for interactivity.

### Save-with-warning on mutations

Schema violations during mutations emit warnings but do not block the write. The mutation succeeds and the file is saved. This is consistent with ADR-001's snapshot validation philosophy — the CLI validates current state, not transitions.

### No auto-commit

All mutations (CLI and UI) update the working tree only. Staging and committing are always explicit user actions, never implicit. The repo is the source of truth, and the user controls when changes enter history.

## Consequences

- The workspace split means domain logic is testable without the CLI or server
- `rust-embed` adds a build step (Vite must run before `cargo build` for the server crate) but keeps deployment simple
- Static and interactive views may drift visually since they don't share templates — acceptable because they serve different audiences
- Renderers are straightforward to add (implement one function from ViewData to output format)
- Runtime field selection (exploring any compatible field on the fly in the live server) is a future enhancement, not part of this initial architecture
