---
id: diagnostic-scope-routing
type: issue
status: in_progress
title: Make diagnostic source-routing structural, not enumerative
parent: code-quality
---

## Motivation

Three consumer sites in the CLI re-derive the same conceptual answer — "what source location does this diagnostic belong to?" — by enumerating every `DiagnosticKind` variant individually:

- `commands/validate.rs::file_for_diagnostic` — maps to a source file path
- `commands/render.rs::invalid_view_ids` — extracts the `view_id` for filtering broken views
- `commands/validate.rs::format_diagnostic_line` — strips item context when a file header already provides it

Each new diagnostic kind requires updates to all three matches. The information being asked for is implicit in each variant's semantics (item-level / file-level / collection-wide / config-level) but isn't represented in the data model.

A second pain point: `workdown validate --format json` emits item-level diagnostics with `item_id` but no file path, and view-level diagnostics with `view_id` but no path either. Downstream tools must re-resolve `workdown-items/` and `config.paths.views` separately to act on a diagnostic. The JSON output is technically self-contained but practically not usable.

## Five scope categories

Every existing variant fits one of five structural relationships to source files:

| Scope | Source data | Variants today |
|---|---|---|
| File | single path | `FileError` |
| Item | single path + item id | `InvalidFieldValue`, `MissingRequired`, `UnknownField`, `BrokenLink`, `RuleViolation`, `AggregateChainConflict`, `AggregateMissingValue` |
| Files | multiple paths | `DuplicateId` |
| Collection | none | `Cycle`, `CountViolation` |
| Config | single path | every `View*` variant; future `Schema*` variants |

These categories are stable structural facts about each variant. The current data model doesn't encode them, leaving consumers to enumerate.

## Design

The fix encodes scope **at the type level**: the variant kind lives inside a scope-tagged outer wrapper, and each wrapper carries the source data appropriate to its scope.

### Top-level shape

```rust
pub struct Diagnostic {
    pub severity: Severity,
    pub body: DiagnosticBody,
}

pub enum DiagnosticBody {
    File(FileDiagnostic),
    Item(ItemDiagnostic),
    Files(FilesDiagnostic),
    Collection(CollectionDiagnostic),
    Config(ConfigDiagnostic),
}
```

### Per-scope wrappers

```rust
pub struct FileDiagnostic {
    pub source_path: PathBuf,
    pub kind: FileDiagnosticKind,
}
pub enum FileDiagnosticKind {
    /// File could not be read or parsed at all.
    Error { detail: String },
}

pub struct ItemDiagnostic {
    pub source_path: PathBuf,
    pub item_id: WorkItemId,
    pub kind: ItemDiagnosticKind,
}
pub enum ItemDiagnosticKind {
    InvalidFieldValue { field: String, detail: FieldValueError },
    MissingRequired { field: String },
    UnknownField { field: String },
    BrokenLink { field: String, target_id: WorkItemId },
    RuleViolation { rule: String, detail: String },
    AggregateChainConflict { field: String, conflicting_ancestor_id: WorkItemId },
    AggregateMissingValue { field: String },
}

pub struct FilesDiagnostic {
    pub paths: Vec<PathBuf>,
    pub kind: FilesDiagnosticKind,
}
pub enum FilesDiagnosticKind {
    DuplicateId { id: WorkItemId },
}

pub struct CollectionDiagnostic {
    pub kind: CollectionDiagnosticKind,
}
pub enum CollectionDiagnosticKind {
    Cycle { field: String, chain: Vec<WorkItemId> },
    CountViolation { rule: String, count: usize, max: Option<u32>, min: Option<u32> },
}

pub struct ConfigDiagnostic {
    pub source_path: PathBuf,
    pub kind: ConfigDiagnosticKind,
}
pub enum ConfigDiagnosticKind {
    ViewDuplicateId { view_id: String },
    ViewMissingSlot { view_id: String, view_type: ViewType, slot: &'static str },
    ViewUnknownField { view_id: String, slot: &'static str, field_name: String },
    // ... 10 more `View*` variants ...
    ViewMetricRowUnknownField { view_id: String, metric_index: usize, slot: &'static str, field_name: String },
    // ... 3 more metric-row variants ...
}
```

Source data hoists to the wrapper where it's invariant for the scope:

- `FileDiagnostic.source_path` (was on `FileError`)
- `ItemDiagnostic.source_path` (was looked up via Store; now structural)
- `ItemDiagnostic.item_id` (hoisted from each item-level variant; the inner variant data drops `item_id`)
- `FilesDiagnostic.paths` (was on `DuplicateId`)
- `ConfigDiagnostic.source_path` (was implicit, looked up from `config.paths.views`)

`view_id` stays on each `ConfigDiagnosticKind` variant — every variant today has one, but a future `Schema*` family won't, so it's not a wrapper-level invariant.

`metric_index` stays on the four metric-row variants — it's variant-specific position data, not scope.

`AggregateMissingValue.leaf_id` is renamed: under the wrapper it becomes `ItemDiagnostic.item_id`. Domain meaning ("this is a tree-leaf") is implied by the kind variant; field naming is uniform.

### Convenience accessors on `Diagnostic`

```rust
impl Diagnostic {
    /// The source file this diagnostic belongs to, if any.
    /// Returns `None` for `Files` (multiple) and `Collection` (none) scopes.
    pub fn source_path(&self) -> Option<&Path> {
        match &self.body {
            DiagnosticBody::File(d) => Some(&d.source_path),
            DiagnosticBody::Item(d) => Some(&d.source_path),
            DiagnosticBody::Config(d) => Some(&d.source_path),
            DiagnosticBody::Files(_) | DiagnosticBody::Collection(_) => None,
        }
    }

    /// The view this diagnostic concerns, if any.
    pub fn view_id(&self) -> Option<&str> {
        if let DiagnosticBody::Config(d) = &self.body {
            view_id_of(&d.kind)
        } else {
            None
        }
    }
}
```

`view_id_of` is a small private helper enumerating the `ConfigDiagnosticKind` variants. That's the single place in the codebase where view-variant enumeration remains.

A `DiagnosticScope` borrowed-projection enum is **not** introduced. The body wrapper is itself the scope abstraction; consumers match on it directly. Adding a parallel `Scope` type would duplicate what `DiagnosticBody` already encodes.

### Display

Display is layered:

- Each inner kind enum (e.g. `ItemDiagnosticKind`) implements `Display` rendering only its own variant data — no `item_id` prefix, no path, no view_id. This is the **compact** form.
- The outer `Diagnostic` implements `Display` orchestrating wrapper-level context: for `Item`, prefixes with `item '{item_id}', `; for `Config`, the inner kind already includes `view '{view_id}'` so no additional prefix; for `File`, paths are usually shown by file headers in `render_human` so the outer Display adds nothing wrapper-level.

`format_diagnostic_line` becomes a one-line dispatch — when grouped under a file header, item-level diagnostics use the inner compact Display directly; everything else uses the outer Display.

### Constructor helpers

To keep emit sites readable:

```rust
impl Diagnostic {
    pub fn item(severity: Severity, source_path: PathBuf, item_id: WorkItemId, kind: ItemDiagnosticKind) -> Self;
    pub fn config(severity: Severity, source_path: PathBuf, kind: ConfigDiagnosticKind) -> Self;
    pub fn file(severity: Severity, source_path: PathBuf, kind: FileDiagnosticKind) -> Self;
    pub fn files(severity: Severity, paths: Vec<PathBuf>, kind: FilesDiagnosticKind) -> Self;
    pub fn collection(severity: Severity, kind: CollectionDiagnosticKind) -> Self;
}
```

Each emit site picks the appropriate constructor; the wrapper structure is built without manual `Diagnostic { severity, body: DiagnosticBody::Item(ItemDiagnostic { ... }) }` ceremony.

### Consumer refactors

```rust
// file_for_diagnostic
fn file_for_diagnostic(diagnostic: &Diagnostic) -> Option<PathBuf> {
    diagnostic.source_path().map(Path::to_path_buf)
}

// invalid_view_ids
fn invalid_view_ids(diagnostics: &[Diagnostic]) -> HashSet<String> {
    diagnostics.iter()
        .filter_map(|diagnostic| diagnostic.view_id().map(str::to_owned))
        .collect()
}

// format_diagnostic_line
fn format_diagnostic_line(diagnostic: &Diagnostic) -> String {
    match &diagnostic.body {
        DiagnosticBody::Item(item) => item.kind.to_string(),  // compact: no item prefix
        _ => diagnostic.to_string(),                           // full
    }
}
```

`file_for_diagnostic` no longer needs a `Store`, `Config`, or `project_root` parameter. It's literally one line.

### Emit-site changes

Each existing emit site picks the appropriate constructor:

- `coerce::coerce_fields` — emits `Diagnostic::item(severity, raw.source_path.clone(), raw.id.clone(), ItemDiagnosticKind::...)`
- `Store::load` — `FileError` → `Diagnostic::file(...)`, `DuplicateId` → `Diagnostic::files(...)`, `BrokenLink` → `Diagnostic::item(...)`
- `cycles::detect_cycles` — `Diagnostic::collection(severity, CollectionDiagnosticKind::Cycle { ... })`
- `rollup::run` — `Diagnostic::item(...)` for chain conflicts, missing values, post-compute required check
- `rules::evaluate` — `Diagnostic::item(...)` for `RuleViolation`, `Diagnostic::collection(...)` for `CountViolation`
- `views_check::evaluate` — `Diagnostic::config(...)` everywhere; takes `views_path: &Path` (threaded via a small `ViewCheckContext { schema, views_path }` struct through the existing helpers)

About 30 emit sites total touched, all mechanical.

### JSON output

The serialized shape changes (not just additively). With `#[serde(flatten)]` on the body wrapper, every diagnostic JSON looks like:

```json
{
  "severity": "error",
  "scope": "config",
  "source_path": "views.yaml",
  "type": "view_unknown_field",
  "view_id": "team-board",
  "slot": "field",
  "field_name": "nonexistent"
}
```

Or for collection-wide:

```json
{
  "severity": "error",
  "scope": "collection",
  "type": "cycle",
  "field": "parent",
  "chain": ["a", "b", "a"]
}
```

Every diagnostic uniformly carries `scope`, the relevant source data (`source_path` or `paths` or omitted), and the variant-specific fields. Downstream tools have a single schema to parse, no special-casing per variant family.

Pre-1.0 with no known external JSON consumers — this is the right window for the shape change. Document in changelog.

## Decisions and rationale

The five questions from the original draft, settled with Option C in mind:

1. **Should `invalid_view_ids` use the same scope mechanism as routing?** No separate `DiagnosticScope` enum is introduced; `DiagnosticBody` is the scope. `view_id()` is a thin convenience method that dispatches into Config and matches its inner kind. Because view variants live together in `ConfigDiagnosticKind`, the view-id enumeration is localized to one private helper.

2. **Does `metric_index` fit into scope?** No. Stays as a regular field on the four metric-row variants — purely Display-private data, not used for routing.

3. **Should item-level diagnostics carry `source_path`?** Yes, structurally. `ItemDiagnostic.source_path` is mandatory at the wrapper level. Same for view-level (`ConfigDiagnostic.source_path`). No defensive Options, no Store lookups, no remember-to-set: the type system requires it where applicable and forbids it where it doesn't apply (Cycle, CountViolation).

4. **JSON output stability.** Shape changes (not additive). Every diagnostic gains `scope` and the relevant source data at a uniform level via flatten. Pre-1.0; documented in changelog.

5. **Justified at current scale?** Yes. The structural restructure delivers wins the variant-level patches couldn't:
   - Compile-time enforcement that each diagnostic carries the right source data.
   - Producer ergonomics: adding a new variant requires picking its scope first — a one-question structural decision instead of "remember to add `source_path`, also update `scope()`, also update `view_id()`."
   - Consumer code collapses: `file_for_diagnostic` becomes a one-liner; `format_diagnostic_line` becomes a four-line body-level match.
   - Single enumerate-every-variant site remains in the codebase (the `view_id_of` helper inside `view_id()`); everything else is body-level dispatch (5 categories).
   - JSON output is uniformly self-describing.

## Caveats

- **Human-readable `workdown validate` output is unchanged.** The compact-vs-full Display split mirrors today's `format_diagnostic_line` behavior; layered Display is mechanics, not UX.
- **Adding a new variant still requires arms in some matches** — but only at the inner-kind level (Display, view_id_of for Config variants). The scope-routing decision itself is structural and enforced.
- **JSON shape is a real break, not additive.** The change is justified by uniform self-description and the pre-1.0 window, but JSON consumers (none external known today) would need updating.
- **Test churn is significant.** Every test that constructs a Diagnostic by hand changes shape. Tests using `matches!(diag.kind, DiagnosticKind::X { .. })` change to nested form. Estimate: 30–50 test sites.

## Decision recorded

A new ADR (`docs/adr/007-diagnostic-scope-typing.md`) captures the structural choice. Future variant additions reference the ADR for the "pick a scope first" pattern.

## Dependencies

- [`diagnostic-variant-cleanup`](diagnostic-variant-cleanup.md) (done) — reduced the View* enum, making the inner `ConfigDiagnosticKind` smaller.

## Out of scope

- Routing `Cycle` per-participating-file (similar to `DuplicateId`'s Files scope) — separate UX decision.
- Changes to severity or emit-side ergonomics beyond the constructor helpers.
- A full `Diagnostic` redesign for a future LSP — speculative; the scope structure naturally accommodates it without further restructure.
- Restructuring `view_id` into a wrapper-level invariant — defer until a non-view config-scope variant family arrives.
