# ADR-007: Diagnostics are scope-typed at the structural level

**Status:** Accepted
**Date:** 2026-05-04

## Context

Every diagnostic produced by the CLI fits one of five relationships to source files: it concerns a single file (`File`), a single work item with a known path (`Item`), multiple files (`Files`), no specific file (`Collection`), or a single config file (`Config`). The original `DiagnosticKind` was a flat tagged-union enum that didn't encode this structural fact: scope was implicit in each variant's semantics but had to be re-derived by every consumer that needed to route, render, or filter diagnostics.

Three consumer sites enumerated the full variant list to extract scope-related answers — file path, view id, item-vs-not category. Adding a new diagnostic variant required updates to all three. Source-file paths for item-level diagnostics were resolved at consumption time via `Store::get(item_id).source_path` rather than carried structurally. Producers had no compile-time enforcement that a diagnostic constructed in a given category carried appropriate source data.

## Decision

`Diagnostic` carries a `body: DiagnosticBody` enum tagged by scope category. Each body variant is a struct holding the source data invariant for that scope plus an inner kind enum holding variant-specific data:

```rust
struct Diagnostic { severity, body }
enum DiagnosticBody {
    File(FileDiagnostic),
    Item(ItemDiagnostic),
    Files(FilesDiagnostic),
    Collection(CollectionDiagnostic),
    Config(ConfigDiagnostic),
}
```

Scope-level invariants (`source_path` for File/Item/Config, `paths` for Files, `item_id` for Item) live on the wrapper. Variant-specific data lives in the inner kind enum. Routing decisions in consumers become body-level matches; the variant kind only matters when the variant data itself does.

## Rationale

Encoding scope structurally turns "where does this diagnostic live?" from a derived question into a property of the type. A `Cycle` cannot carry a `source_path` because `CollectionDiagnostic` has no field for it. An `InvalidFieldValue` cannot be constructed without a `source_path` because `ItemDiagnostic.source_path` is non-optional. Producers no longer need to remember to set the path; consumers no longer pattern-match every variant to find one.

A flatter alternative — adding `source_path` to each individual variant — was considered. It buys per-variant uniformity but leaves the invariants documentary rather than enforced, requires duplication across 24 variants, and leaves consumers still doing per-variant matches to find the path. The structural restructure is roughly the same edit footprint with stronger guarantees and cleaner consumer code.

## Consequences

- Adding a new variant is a two-step structural decision: pick a scope category (which wrapper), then add the variant to that wrapper's inner kind enum.
- Source data is carried by every diagnostic that has any — no Store lookups required at consumption time.
- JSON output is uniformly self-describing: every diagnostic carries `scope` plus the relevant source data at a uniform level (via `serde(flatten)`).
- A single enumerate-every-view-variant site remains (a private helper inside `view_id()`); other consumer-side enumerations collapse to 5-arm body-level matches.
- Display becomes layered: each inner kind renders compact; outer `Diagnostic` Display orchestrates wrapper-level prefixes.
- JSON shape is a non-additive change. Pre-1.0 with no known external consumers; documented in the changelog.
