---
id: workspace-refactor
type: issue
status: to_do
title: Split into core / cli / server workspace
parent: foundation
---

Convert the project from a single crate to a Cargo workspace with three crates:

```
crates/
  core/     # pure library: parse, validate, query, mutate, schema loading
  cli/      # thin binary; clap subcommands call into core
  server/   # axum-based web server (library crate, stub at this point)
```

Also scaffold `ui/` at the repo root for the Svelte + TS frontend, even though it's empty — confirms the build integration plan is viable.

## Scope

- Move domain logic from `src/` into `crates/core/src/`
- `crates/cli/src/main.rs` becomes a thin wrapper calling `core::*`
- `crates/server/` is a library crate with a single `serve()` function that prints "not yet implemented" — actual endpoints land in the `server` milestone
- `cargo build --workspace` produces one binary (the CLI)
- Existing tests continue to pass

## Out of scope

- Actual server functionality
- Frontend build integration beyond scaffolding — wiring comes in `serve-command-skeleton`
