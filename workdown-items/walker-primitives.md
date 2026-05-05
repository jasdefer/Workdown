---
id: walker-primitives
type: issue
status: done
title: Unify the upward chain walks and link-target reads
parent: code-quality
---

Two related patterns repeated across the codebase — both prerequisites of any graph operation, both reimplemented at every call site.

## Pattern 1 — single-`Link` upward chain walks

Several places walk a `link` field upward from a starting item to the chain root, accumulating ancestors along the way. The walks share the same skeleton (visited-set + read field + follow Link + terminate on cycle/missing) but each call site rolls its own loop.

Sites:

- `store/rollup.rs` — up-walk pass and `covered` (two walks)
- `view_data/gantt_by_initiative.rs::walk_to_root`
- `view_data/gantt_by_depth.rs::walk_to_depth`

(`store/cycles.rs::dfs` is a full-graph DFS with white/gray/black coloring over `Link`+`Links` — different shape, out of scope.)

## Pattern 2 — "what does this item link to via field X?"

Read a field as a list of referenced ids: `Link` → `[t]`, `Links` → its full vec, anything else → empty. Five-plus inline matches doing exactly this:

- `store/cycles.rs::targets`
- `view_data/graph.rs` edge collection
- `view_data/gantt.rs::predecessor_ids`
- `store/mod.rs` reverse-link build
- `store/rollup.rs::parent_of` (single-Link variant of the same idea)

## Objective

A small set of primitives next to the model that capture both patterns, so every call site supplies only what makes it different (field name + per-step work or per-item shape). Cycle handling, missing-target handling, and the Link/Links match logic each live in exactly one place.

## Out of scope

- Full-graph cycle detection (`cycles.rs::dfs`) — different problem.
- Downward forest walk (`view_data/traverse.rs::walk_forest`) — already a shared module within view_data.
- BFS predecessor closure / topological sort (`gantt.rs::collect_after_closure`) — single user.
- Changing observable behavior of any caller (cycle reporting, aggregate semantics, gantt grouping).
