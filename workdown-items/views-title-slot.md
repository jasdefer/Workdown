---
id: views-title-slot
type: issue
status: to_do
title: Add per-view `title:` slot to views.yaml
parent: renderers
depends_on: []
---

Add an optional `title:` slot to every view entry in views.yaml. Its value
is a field name on the schema whose value each rendered item (card, row,
node, bar) uses as its display title. Omitted → renderers fall back to
item id.

## Motivation

Renderers and the live server need a human-readable label for each item
beyond the id. The `title` field in the default schema is a convention,
not a requirement — users can rename or remove it. A per-view slot lets
each view declare which field provides its titles without the CLI
assuming a schema convention.

Per-view (rather than project-wide config) because view-semantic config
belongs in views.yaml, alongside slots like `field:`, `start:`, `end:`.
Generalizing into a top-level default is deferred until the boilerplate
of repeating `title: title` on every view becomes painful.

## Scope

### 1. Parse the slot onto `View`

`title` is cross-cutting — every view type accepts it, same meaning
everywhere. Add it as a shared field on `View`, next to `where_clauses`
— *not* repeated inside each `ViewKind` variant.

`crates/core/src/model/views.rs`:

```rust
pub struct View {
    pub id: String,
    pub where_clauses: Vec<String>,
    pub title: Option<String>,   // ← new
    pub kind: ViewKind,
}
```

`crates/core/src/parser/views.rs` — accept `title:` on every view entry
(YAML is flat; the parser just reads it once per view and writes it to
`View.title`).

### 2. Validate the slot

`crates/core/src/views_check.rs` — add a check:

- If `title:` is set, the referenced name must resolve in the schema.
  `id` is accepted though redundant.
- The referenced field must be one of: `string`, `choice`. Reject
  `link`, `links`, `multichoice`, `boolean`, `integer`, `float`, `date`,
  `list` — these need formatting decisions beyond what a one-liner title
  can express. (Revisit the type list during implementation if any feel
  too strict.)

### 3. JSON schema

`crates/core/defaults/views.schema.json` — add the optional `title`
property. Because it applies to every view type uniformly, the cleanest
place is the shared base schema that each type branch extends (or the
equivalent construct in the current schema shape).

### 4. Documentation

`docs/views.md` — add `title:` to the optional-slots description; add a
short prose section describing the slot, the fallback-to-id behaviour,
and the allowed field types.

### 5. Tests

- Parser accepts `title:` on every view type.
- `views_check` rejects `title:` pointing at a missing field.
- `views_check` rejects `title:` pointing at an incompatible field type.
- Round-trip: a views.yaml file with `title:` set parses and re-emits
  unchanged.

## Acceptance

- `title: <field>` is accepted on every view type in views.yaml.
- Invalid `title:` references surface as diagnostics from
  `workdown validate`.
- The views.json schema covers the new slot so editor autocomplete picks
  it up.
- `docs/views.md` documents the slot.

## Out of scope

- Top-level `default_title:` inheritance — deferred until boilerplate
  justifies it.
- `view-data-intermediate` consuming the slot — that's the next issue
  down the chain; this one just adds and validates the field.
- Changing existing default views in `crates/core/defaults/views.yaml`
  to set `title:` — separate editorial choice; not needed for this to
  land.
