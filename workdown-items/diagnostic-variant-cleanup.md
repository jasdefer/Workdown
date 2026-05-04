---
id: diagnostic-variant-cleanup
type: issue
status: done
title: Collapse parallel View* slot variants and unify their validation helpers
parent: code-quality
---

Four `views.yaml` slots — `graph.group_by`, `gantt.after`, `gantt_by_initiative.root_link`, `gantt_by_depth.depth_link` — share the same validation rules (must be a `link`/`links` field, `allow_cycles: false`, not an inverse name). Today this is implemented as four near-identical validation helpers and eight near-identical diagnostic variants. Unify both.

## Context

The four helpers in `crates/core/src/views_check.rs`:

- `check_graph_group_by` (line 414) — Link only
- `check_after_slot` (line 657) — Link or Links
- `check_root_link_slot` (line 552) — Link only
- `check_depth_link_slot` (line 602) — Link only

All have the same shape:

```
1. field not in schema?
   - is inverse name? → emit *_InverseNotAllowed
   - else            → emit ViewUnknownField { slot }
2. type matches Link/Links?
   - allow_cycles wrong? → emit *_Cyclic
3. else → emit ViewFieldTypeMismatch { slot }
```

The eight parallel `DiagnosticKind` variants in `crates/core/src/model/diagnostic.rs`:

- `ViewGroupByCyclic` / `ViewGanttAfterCyclic` / `ViewGanttRootLinkCyclic` / `ViewGanttDepthLinkCyclic`
- `ViewGroupByInverseNotAllowed` / `ViewGanttAfterInverseNotAllowed` / `ViewGanttRootLinkInverseNotAllowed` / `ViewGanttDepthLinkInverseNotAllowed`

Each pair carries identical fields (`view_id`, `field_name`); the only difference is which slot the diagnostic is about.

Consumers ripple this duplication:

- Display impl in `model/diagnostic.rs` — 8 nearly identical match arms
- `commands/render.rs::invalid_view_ids` — 8 OR-pattern arms
- `commands/validate.rs::file_for_diagnostic` — 8 OR-pattern arms

## Scope

### Phase A — Unify validation helpers

Replace the four helpers with one parameterized helper:

```rust
fn check_link_slot(
    schema: &Schema,
    view_id: &str,
    slot: &'static str,
    field_name: &str,
    allow_links: bool,   // true only for `after`
    out: &mut Vec<Diagnostic>,
)
```

Call sites: `ViewKind::Graph::group_by`, `ViewKind::Gantt::after`, `ViewKind::GanttByInitiative::root_link`, `ViewKind::GanttByDepth::depth_link`.

This phase is internal to `views_check.rs`. Still emits the eight existing variant pairs — no observable change.

### Phase B — Collapse parallel variants

Replace the 8 variants with 2:

```rust
DiagnosticKind::ViewSlotCyclic            { view_id, slot, field_name }
DiagnosticKind::ViewSlotInverseNotAllowed { view_id, slot, field_name }
```

Preserve message specificity via a small lookup helper, e.g.:

```rust
fn slot_purpose(slot: &str) -> &'static str {
    match slot {
        "group_by"   => "to be used for subgraph nesting",
        "after"      => "to be used for predecessor resolution",
        "root_link"  => "to be used for initiative partitioning",
        "depth_link" => "to be used for depth partitioning",
        _ => "",
    }
}
```

Updates required in:

- `model/diagnostic.rs` — variant defs + Display impl (8 arms → 2)
- `views_check.rs` — emit sites (already centralized after Phase A)
- `commands/render.rs::invalid_view_ids` — 8 arms → 2
- `commands/validate.rs::file_for_diagnostic` — 8 arms → 2
- Tests in `crates/core/tests/validate_views.rs` — assertions on collapsed variants

JSON output: tag names change from `view_group_by_cyclic` etc. to `view_slot_cyclic`. The `slot` field becomes part of the payload. No external JSON consumers exist today, but call this out in the change.

## Acceptance

- One `check_link_slot` helper covers all four call sites
- Two variants (`ViewSlotCyclic`, `ViewSlotInverseNotAllowed`) replace the eight
- Human-readable error messages unchanged before/after on test fixtures
- `cargo build --workspace` and `cargo test --workspace` pass
- `workdown validate` exit codes and human output unchanged on the dogfood project

## Considered — handled in a separate issue

- **Routing match repetition** (`file_for_diagnostic`, `invalid_view_ids` enumerating every variant) — broader than this slot-pair pattern; tracked under [`diagnostic-scope-routing`](diagnostic-scope-routing.md).
- **Metric-row variant duplication** (`ViewMetricRowUnknownField` etc. mirroring `ViewUnknownField` plus `metric_index`) — different shape (context tagging vs. parallel slots); revisit when a third context appears.

## Out of scope

- Any change to non-View diagnostics (item / file / collection scopes)
- Restructuring the flat `DiagnosticKind` enum into nested per-scope enums
- Severity-handling changes
