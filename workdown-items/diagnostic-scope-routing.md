---
id: diagnostic-scope-routing
type: issue
status: to_do
title: Make diagnostic source-routing structural, not enumerative
parent: code-quality
---

**Status: thinking-in-progress.** This issue captures an observation and a candidate direction; it does not yet specify an implementation. Open questions below need to be settled before this is ready to pick up.

## Observation

Three consumer sites in the CLI re-derive the same conceptual answer — "what source location does this diagnostic belong to?" — by enumerating every `DiagnosticKind` variant individually:

- `commands/validate.rs::file_for_diagnostic` (~60 lines, every variant) — maps to a source file path
- `commands/render.rs::invalid_view_ids` (22-arm OR pattern across `View*` variants) — extracts the `view_id` for filtering
- `commands/validate.rs::format_diagnostic_line` (cherry-picks ~7 variants) — strips item context when a file header already provides it

Each new diagnostic kind requires updates to all three matches. The information being asked for is implicit in each variant's semantics (is this an item-level, collection-level, file-level, or config-level finding?) but isn't represented in the data model.

After [`diagnostic-variant-cleanup`](diagnostic-variant-cleanup.md) lands, the `View*` enumeration shrinks but the underlying enumerate-every-variant pattern remains.

## Categorization

Each existing variant has one of five relationships to source files:

| Relationship | Examples | What's known |
|---|---|---|
| Single file, path carried directly | `FileError` | path in variant |
| Single file, path looked up via id | `InvalidFieldValue`, `MissingRequired`, `UnknownField`, `BrokenLink`, `RuleViolation`, `Aggregate*` | path lives on the item; reconstructed via `store.get(id).source_path` |
| Multiple files (intrinsic) | `DuplicateId { paths }` | already carries a list |
| No single file (collection-wide) | `Cycle`, `CountViolation` | no path concept |
| Always one config file | every `View*` variant | implicit — every consumer hardcodes `config.paths.views` |

A future `Schema*` family for cross-file schema validation would add a sixth case: always pointing at `schema.yaml`.

## Direction under consideration

A method on `Diagnostic` that returns a structured description of source location:

```rust
enum DiagnosticScope<'a> {
    File(&'a Path),
    Files(&'a [PathBuf]),
    Item(&'a WorkItemId),
    Collection,
    Config(&'a Path),  // views.yaml today; schema.yaml later
}

impl Diagnostic {
    fn scope(&self) -> DiagnosticScope<'_> { /* one match, lives next to definitions */ }
}
```

Consumer code becomes:

```rust
match diagnostic.scope() {
    DiagnosticScope::File(p) | DiagnosticScope::Config(p) => Some(p.to_path_buf()),
    DiagnosticScope::Item(id) => store.get(id.as_str()).map(|i| i.source_path.clone()),
    DiagnosticScope::Files(_) | DiagnosticScope::Collection => None,
}
```

The match still exists but lives once, next to the variant definitions, so adding a variant naturally surfaces the routing decision.

## Open questions

These should be settled before implementing:

1. **Should `invalid_view_ids` use the same scope mechanism, or a separate concept?** "Which view does this belong to" is a slightly different question than "which file." Possibly `DiagnosticScope::Config { path, view_id: Option<&str> }`, or a separate `view_id()` method. Needs design.

2. **Does the metric-row context fit into scope, or sit alongside it?** Metric-row variants are scoped to `views.yaml` *and* a specific row within a view. If scope encodes only the file, metric_index is orthogonal. If scope encodes view + row, it conflates concerns.

3. **Is there a parallel item-level concern worth solving at the same time?** `WorkItemId` → source path lookup happens via the Store. Should item-level diagnostics carry their `source_path` directly so they're self-contained for non-Store consumers (JSON, future server, future LSP)? This is independent of the scope-method idea but the questions interact.

4. **JSON output stability.** `Diagnostic` serializes as a tagged union today. Adding `scope()` doesn't change serialization — it's a method, not a field. But if open question #3 leads to adding `source_path` to item-level variants, the JSON shape changes for those. Worth deciding whether to bundle.

5. **Is the cost actually paid back at current scale?** Five consumer sites that each enumerate 30 variants is a real cost. But it's stable cost — adding a new diagnostic doesn't require changing the *shape* of those matches, only adding an arm. Worth verifying that the structural change is justified by a concrete pain point (likely: web server diagnostic display, JSON tooling), not just aesthetic improvement.

## Dependencies

Should land **after** [`diagnostic-variant-cleanup`](diagnostic-variant-cleanup.md):
- Phase B of that issue collapses 16 variants to 4. The scope-method's match is smaller and easier to write against the cleaned-up enum.
- No reason to design routing logic against variants we're about to delete.

## Out of scope

- Anything `diagnostic-variant-cleanup` already covers
- Restructuring the flat enum into nested per-scope enums (the foundation-cleanup-deferred work; revisit only if a third "context" appears)
- Changes to severity or to producer-side emit ergonomics
