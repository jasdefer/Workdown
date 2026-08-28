# ADR-002: Generic field types drive behavior

**Status:** Accepted
**Date:** 2026-04-10

## Context

The CLI needs to understand certain fields specially (e.g., for board views, tree views, dependency graphs). The question is whether specific field names are hardcoded or whether field types drive behavior generically.

## Decision

Field types determine available behaviors, not field names. The type system defines categories (enum, reference, integer, date, string, etc.) and the CLI operates on types generically. For example, any enum field can be rendered as a board, any reference field as a tree or graph.

## Consequences

- Users can define multiple enum fields (status, priority, sprint) and visualize any of them as a board
- Users can define multiple reference fields (parent, epic, team) and render any as a tree
- CLI commands use `--field` flags with sensible defaults from config
- No field name is "magic" — the schema is the single source of truth

## The two named exceptions

Two field names are known to core, and both are deliberate:

- **`id`** is identity, not data. It is resolved from the filename (or
  an explicit `id:` key) before any field is coerced, and it is what
  every relation points at. It could not be a schema field without
  making the schema define its own addressing.
- **`title`** is the source `workdown add` slugifies into a filename
  when no `id` is given (`operations/add.rs`), matching the title
  fallback that prettifies a filename back into a title when no
  `title` is set. This is a naming convention for new files, not a
  behavior: nothing about how a `title` field is validated, rendered,
  or filtered differs from any other string field. Should a project
  ever need a different slug source, the fix is a config key naming
  the field — not a second special case.

No other name is consulted. Board, tree, and graph fields are named in
`config.yaml`; display roles are named in `views.yaml`.
