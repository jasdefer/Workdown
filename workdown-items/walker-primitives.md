---
id: walker-primitives
type: issue
status: to_do
title: Unify the four single-Link upward chain walks
parent: code-quality
---

Several places walk a `link` field upward from a starting item to the chain root, accumulating ancestors along the way. The walks are structurally the same — follow `FieldValue::Link` on field `X` until it terminates or revisits a node — but each call site rolls its own loop with its own visited-set handling, termination checks, and chain-collection semantics.

Known sites:

- `crates/core/src/store/rollup.rs` — parent-chain walk for aggregate rollup
- `crates/core/src/store/cycles.rs` — cycle detection over arbitrary `link` fields
- `crates/core/src/view_data/gantt_by_initiative.rs` — `root_link` walk to find each item's top-level ancestor
- `crates/core/src/view_data/gantt_by_depth.rs` — `depth_link` walk to compute depth

## Objective

Extract a small primitive (or a short family of them) that captures the walk and lets each call site supply only what makes it different — typically the field name and what to do with each visited node. The four sites should share one implementation of "follow this link upward" with consistent cycle handling.

## Out of scope

- `links` (multi) traversal — that's a graph walk, different shape.
- Changing observable behavior of any caller (cycle reporting, aggregate semantics, gantt grouping).
