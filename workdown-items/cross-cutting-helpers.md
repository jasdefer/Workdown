---
id: cross-cutting-helpers
type: issue
status: to_do
title: Relocate cross-cutting helpers out of feature modules
parent: code-quality
---

Some helpers that everything-needs-to-format-a-value live inside feature modules where they happened to be written first. They're now imported across the codebase, which makes the dependency graph misleading and suggests the helper is feature-specific when it isn't.

Known case:

- `format_field_value` lives in `crates/core/src/query/format.rs` but is used by renderers, rules, and view_data extractors. The query module is a consumer of the formatter, not its owner.

## Objective

Move helpers that operate on core model types and are used outside their current home into a location that reflects their role (e.g. alongside the type they format, or in a small dedicated module). Audit for similar cases while doing this — the goal is to fix the misleading dependency graph, not to chase one specific helper.

## Out of scope

- Changing what any helper does or how it formats output.
- Restructuring whole modules — this is about moving a small number of free functions to where they belong.
