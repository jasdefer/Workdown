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
